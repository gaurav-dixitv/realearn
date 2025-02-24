use crate::application::{
    ActivationConditionModel, ActivationType, BankConditionModel, ModifierConditionModel,
};
use crate::base::default_util::is_default;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationConditionData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub activation_type: ActivationType,
    #[serde(default, skip_serializing_if = "is_default")]
    pub modifier_condition_1: ModifierConditionModel,
    #[serde(default, skip_serializing_if = "is_default")]
    pub modifier_condition_2: ModifierConditionModel,
    #[serde(default, skip_serializing_if = "is_default")]
    pub program_condition: BankConditionModel,
    #[serde(default, skip_serializing_if = "is_default")]
    pub eel_condition: String,
}

impl ActivationConditionData {
    pub fn from_model(model: &ActivationConditionModel) -> ActivationConditionData {
        ActivationConditionData {
            activation_type: model.activation_type.get(),
            modifier_condition_1: model.modifier_condition_1.get(),
            modifier_condition_2: model.modifier_condition_2.get(),
            program_condition: model.bank_condition.get(),
            eel_condition: model.eel_condition.get_ref().clone(),
        }
    }

    pub fn apply_to_model(&self, model: &mut ActivationConditionModel, with_notification: bool) {
        model
            .activation_type
            .set_with_optional_notification(self.activation_type, with_notification);
        model
            .modifier_condition_1
            .set_with_optional_notification(self.modifier_condition_1, with_notification);
        model
            .modifier_condition_2
            .set_with_optional_notification(self.modifier_condition_2, with_notification);
        model
            .bank_condition
            .set_with_optional_notification(self.program_condition, with_notification);
        model
            .eel_condition
            .set_with_optional_notification(self.eel_condition.clone(), with_notification);
    }
}
