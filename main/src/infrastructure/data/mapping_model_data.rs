use crate::application::MappingModel;
use crate::base::default_util::{bool_true, is_bool_true, is_default};
use crate::domain::{
    ExtendedProcessorContext, FeedbackSendBehavior, GroupId, GroupKey, MappingCompartment,
    MappingKey, Tag,
};
use crate::infrastructure::data::{
    ActivationConditionData, DataToModelConversionContext, EnabledData, MigrationDescriptor,
    ModeModelData, ModelToDataConversionContext, SourceModelData, TargetModelData,
};
use crate::infrastructure::plugin::App;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingModelData {
    // Saved since ReaLearn 1.12.0, doesn't have to be a UUID since 2.11.0-pre.13 and corresponds
    // to the model *key* instead!
    #[serde(default, skip_serializing_if = "is_default")]
    pub id: Option<MappingKey>,
    /// Saved only in some ReaLearn 2.11.0-pre-releases. Later we persist this in "id" field again.
    /// So this is just for being compatible with those few pre-releases!
    #[serde(default, skip_serializing)]
    pub key: Option<MappingKey>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tags: Vec<Tag>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub group_id: GroupKey,
    pub source: SourceModelData,
    pub mode: ModeModelData,
    pub target: TargetModelData,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    pub is_enabled: bool,
    #[serde(flatten)]
    pub enabled_data: EnabledData,
    #[serde(flatten)]
    pub activation_condition_data: ActivationConditionData,
    #[serde(default, skip_serializing_if = "is_default")]
    pub prevent_echo_feedback: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub send_feedback_after_control: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub advanced: Option<serde_yaml::mapping::Mapping>,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    pub visible_in_projection: bool,
}

impl MappingModelData {
    pub fn from_model(
        model: &MappingModel,
        conversion_context: &impl ModelToDataConversionContext,
    ) -> MappingModelData {
        MappingModelData {
            id: Some(model.key().clone()),
            key: None,
            name: model.name.get_ref().clone(),
            tags: model.tags.get_ref().clone(),
            group_id: {
                conversion_context
                    .group_key_by_id(model.group_id.get())
                    .unwrap_or_default()
            },
            source: SourceModelData::from_model(&model.source_model),
            mode: ModeModelData::from_model(&model.mode_model),
            target: TargetModelData::from_model(&model.target_model, conversion_context),
            is_enabled: model.is_enabled.get(),
            enabled_data: EnabledData {
                control_is_enabled: model.control_is_enabled.get(),
                feedback_is_enabled: model.feedback_is_enabled.get(),
            },
            prevent_echo_feedback: model.feedback_send_behavior.get()
                == FeedbackSendBehavior::PreventEchoFeedback,
            send_feedback_after_control: model.feedback_send_behavior.get()
                == FeedbackSendBehavior::SendFeedbackAfterControl,
            activation_condition_data: ActivationConditionData::from_model(
                &model.activation_condition_model,
            ),
            advanced: model.advanced_settings().cloned(),
            visible_in_projection: model.visible_in_projection.get(),
        }
    }

    pub fn to_model(
        &self,
        compartment: MappingCompartment,
        context: ExtendedProcessorContext,
        conversion_context: &impl DataToModelConversionContext,
    ) -> MappingModel {
        self.to_model_flexible(
            compartment,
            Some(context),
            &MigrationDescriptor::default(),
            Some(App::version()),
            conversion_context,
        )
    }

    /// Use this for integrating the resulting model into a preset.
    pub fn to_model_for_preset(
        &self,
        compartment: MappingCompartment,
        migration_descriptor: &MigrationDescriptor,
        preset_version: Option<&Version>,
        conversion_context: &impl DataToModelConversionContext,
    ) -> MappingModel {
        self.to_model_flexible(
            compartment,
            // We don't need the context because additional track/FX properties don't
            // need to be resolved when just creating a preset.
            None,
            migration_descriptor,
            preset_version,
            conversion_context,
        )
    }

    /// The context - if available - will be used to resolve some track/FX properties for UI
    /// convenience. The context is necessary if there's the possibility of loading data saved with
    /// ReaLearn < 1.12.0.
    pub fn to_model_flexible(
        &self,
        compartment: MappingCompartment,
        context: Option<ExtendedProcessorContext>,
        migration_descriptor: &MigrationDescriptor,
        preset_version: Option<&Version>,
        conversion_context: &impl DataToModelConversionContext,
    ) -> MappingModel {
        let key: MappingKey = self
            .key
            .clone()
            .or_else(|| self.id.clone())
            .unwrap_or_else(MappingKey::random);
        // Preliminary group ID
        let mut model = MappingModel::new(compartment, GroupId::default(), key);
        self.apply_to_model_internal(
            &mut model,
            context,
            migration_descriptor,
            preset_version,
            false,
            conversion_context,
        );
        model
    }

    /// This is for realtime mapping modification (with notification, no ID changes), e.g. for copy
    /// & paste within one ReaLearn version.
    pub fn apply_to_model(
        &self,
        model: &mut MappingModel,
        context: ExtendedProcessorContext,
        conversion_context: &impl DataToModelConversionContext,
    ) {
        self.apply_to_model_internal(
            model,
            Some(context),
            &MigrationDescriptor::default(),
            Some(App::version()),
            true,
            conversion_context,
        );
    }

    /// The processor context - if available - will be used to resolve some track/FX properties for
    /// UI convenience. The context is necessary if there's the possibility of loading data saved
    /// with ReaLearn < 1.12.0.
    fn apply_to_model_internal(
        &self,
        model: &mut MappingModel,
        processor_context: Option<ExtendedProcessorContext>,
        migration_descriptor: &MigrationDescriptor,
        preset_version: Option<&Version>,
        with_notification: bool,
        conversion_context: &impl DataToModelConversionContext,
    ) {
        model
            .name
            .set_with_optional_notification(self.name.clone(), with_notification);
        model
            .tags
            .set_with_optional_notification(self.tags.clone(), with_notification);
        let group_id = conversion_context
            .group_id_by_key(&self.group_id)
            .unwrap_or_default();
        model
            .group_id
            .set_with_optional_notification(group_id, with_notification);
        self.activation_condition_data.apply_to_model(
            model.activation_condition_model.borrow_mut(),
            with_notification,
        );
        let compartment = model.compartment();
        self.source.apply_to_model_flexible(
            model.source_model.borrow_mut(),
            with_notification,
            compartment,
            preset_version,
        );
        self.mode.apply_to_model_flexible(
            model.mode_model.borrow_mut(),
            migration_descriptor,
            &self.name,
            with_notification,
        );
        self.target.apply_to_model_flexible(
            model.target_model.borrow_mut(),
            processor_context,
            preset_version,
            with_notification,
            compartment,
            conversion_context,
        );
        model
            .is_enabled
            .set_with_optional_notification(self.is_enabled, with_notification);
        model.control_is_enabled.set_with_optional_notification(
            self.enabled_data.control_is_enabled,
            with_notification,
        );
        model.feedback_is_enabled.set_with_optional_notification(
            self.enabled_data.feedback_is_enabled,
            with_notification,
        );
        let feedback_send_behavior = if self.prevent_echo_feedback {
            // Took precedence if both checkboxes were ticked (was possible in ReaLearn < 2.10.0).
            FeedbackSendBehavior::PreventEchoFeedback
        } else if self.send_feedback_after_control {
            FeedbackSendBehavior::SendFeedbackAfterControl
        } else {
            FeedbackSendBehavior::Normal
        };
        model
            .feedback_send_behavior
            .set_with_optional_notification(feedback_send_behavior, with_notification);
        let _ = model.set_advanced_settings(self.advanced.clone(), with_notification);
        model
            .visible_in_projection
            .set_with_optional_notification(self.visible_in_projection, with_notification);
    }
}
