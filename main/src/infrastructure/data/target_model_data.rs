use super::f32_as_u32;
use super::none_if_minus_one;
use reaper_high::{BookmarkType, Fx, Guid, Reaper};

use crate::application::{
    AutomationModeOverrideType, BookmarkAnchorType, FxParameterPropValues, FxPropValues,
    FxSnapshot, RealearnAutomationMode, RealearnTrackArea, TargetCategory, TargetModel, TargetUnit,
    TrackPropValues, TrackRoutePropValues, TrackRouteSelectorType, VirtualControlElementType,
    VirtualFxParameterType, VirtualFxType, VirtualTrackType,
};
use crate::base::default_util::{bool_true, is_bool_true, is_default, is_none_or_some_default};
use crate::base::notification;
use crate::domain::{
    get_fx_chain, ActionInvocationType, AnyOnParameter, Exclusivity, ExtendedProcessorContext,
    FxDisplayType, GroupKey, MappingCompartment, OscDeviceId, ReaperTargetType, SeekOptions,
    SendMidiDestination, SoloBehavior, Tag, TouchedParameterType, TrackExclusivity, TrackRouteType,
    TransportAction, VirtualTrack,
};
use crate::infrastructure::data::{
    DataToModelConversionContext, ModelToDataConversionContext, VirtualControlElementIdData,
};
use crate::infrastructure::plugin::App;
use helgoboss_learn::OscTypeTag;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetModelData {
    #[serde(default, skip_serializing_if = "is_default")]
    pub category: TargetCategory,
    #[serde(default, skip_serializing_if = "is_default")]
    pub unit: TargetUnit,
    // reaper_type would be a better name but we need backwards compatibility
    #[serde(default, skip_serializing_if = "is_default")]
    pub r#type: ReaperTargetType,
    // Action target
    #[serde(default, skip_serializing_if = "is_default")]
    pub command_name: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub invocation_type: ActionInvocationType,
    // Until ReaLearn 1.0.0-beta6
    #[serde(default, skip_serializing)]
    pub invoke_relative: Option<bool>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub with_track: bool,
    // Track target
    #[serde(flatten)]
    pub track_data: TrackData,
    #[serde(default, skip_serializing_if = "is_default")]
    pub enable_only_if_track_is_selected: bool,
    // FX target
    #[serde(flatten)]
    pub fx_data: FxData,
    #[serde(default, skip_serializing_if = "is_default")]
    pub enable_only_if_fx_has_focus: bool,
    // Track route target
    #[serde(flatten)]
    pub track_route_data: TrackRouteData,
    // FX parameter target
    #[serde(flatten)]
    pub fx_parameter_data: FxParameterData,
    // Track selection target (replaced with `track_exclusivity` since v2.4.0)
    #[serde(default, skip_serializing_if = "is_default")]
    pub select_exclusively: Option<bool>,
    // Track solo target (since v2.4.0, also changed default from "ignore routing" to "in place")
    #[serde(default, skip_serializing_if = "is_none_or_some_default")]
    pub solo_behavior: Option<SoloBehavior>,
    // Toggleable track targets (since v2.4.0)
    #[serde(default, skip_serializing_if = "is_default")]
    pub track_exclusivity: TrackExclusivity,
    // Transport target
    #[serde(default, skip_serializing_if = "is_default")]
    pub transport_action: TransportAction,
    // Any-on target
    #[serde(default, skip_serializing_if = "is_default")]
    pub any_on_parameter: AnyOnParameter,
    #[serde(default, skip_serializing_if = "is_default")]
    pub control_element_type: VirtualControlElementType,
    #[serde(default, skip_serializing_if = "is_default")]
    pub control_element_index: VirtualControlElementIdData,
    #[serde(default, skip_serializing_if = "is_default")]
    pub fx_snapshot: Option<FxSnapshot>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub touched_parameter_type: TouchedParameterType,
    // Bookmark target
    #[serde(flatten)]
    pub bookmark_data: BookmarkData,
    // Seek target
    #[serde(flatten)]
    pub seek_options: SeekOptions,
    // Track show target
    #[serde(default, skip_serializing_if = "is_default")]
    pub track_area: RealearnTrackArea,
    // Track automation mode target
    #[serde(default, skip_serializing_if = "is_default")]
    pub track_automation_mode: RealearnAutomationMode,
    // Automation mode override target
    #[serde(default, skip_serializing_if = "is_default")]
    pub automation_mode_override_type: AutomationModeOverrideType,
    // FX Open and FX Navigate target
    #[serde(default, skip_serializing_if = "is_default")]
    pub fx_display_type: FxDisplayType,
    // Track selection related targets
    #[serde(default, skip_serializing_if = "is_default")]
    pub scroll_arrange_view: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub scroll_mixer: bool,
    // Send MIDI
    #[serde(default, skip_serializing_if = "is_default")]
    pub send_midi_destination: SendMidiDestination,
    #[serde(default, skip_serializing_if = "is_default")]
    pub raw_midi_pattern: String,
    // Send OSC
    #[serde(default, skip_serializing_if = "is_default")]
    pub osc_address_pattern: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub osc_arg_index: Option<u32>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub osc_arg_type: OscTypeTag,
    #[serde(default, skip_serializing_if = "is_default")]
    pub osc_dev_id: Option<OscDeviceId>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub slot_index: usize,
    #[serde(default, skip_serializing_if = "is_default")]
    pub next_bar: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub buffered: bool,
    #[serde(default = "bool_true", skip_serializing_if = "is_bool_true")]
    pub poll_for_feedback: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tags: Vec<Tag>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub exclusivity: Exclusivity,
    #[serde(default, skip_serializing_if = "is_default")]
    pub group_id: GroupKey,
    #[serde(default, skip_serializing_if = "is_default")]
    pub active_mappings_only: bool,
}

impl TargetModelData {
    pub fn from_model(
        model: &TargetModel,
        conversion_context: &impl ModelToDataConversionContext,
    ) -> Self {
        Self {
            category: model.category.get(),
            unit: model.unit.get(),
            r#type: model.r#type.get(),
            command_name: model
                .action
                .get_ref()
                .as_ref()
                .map(|a| match a.command_name() {
                    // Built-in actions don't have a command name but a persistent command ID.
                    // Use command ID as string.
                    None => a.command_id().to_string(),
                    // ReaScripts and custom actions have a command name as persistent identifier.
                    Some(name) => name.into_string(),
                }),
            invocation_type: model.action_invocation_type.get(),
            // Not serialized anymore because deprecated
            invoke_relative: None,
            track_data: serialize_track(model.track()),
            enable_only_if_track_is_selected: model.enable_only_if_track_selected.get(),
            with_track: model.with_track.get(),
            fx_data: serialize_fx(model.fx()),
            enable_only_if_fx_has_focus: model.enable_only_if_fx_has_focus.get(),
            track_route_data: serialize_track_route(model.track_route()),
            fx_parameter_data: serialize_fx_parameter(model.fx_parameter()),
            select_exclusively: None,
            solo_behavior: Some(model.solo_behavior.get()),
            track_exclusivity: model.track_exclusivity.get(),
            transport_action: model.transport_action.get(),
            any_on_parameter: model.any_on_parameter.get(),
            control_element_type: model.control_element_type.get(),
            control_element_index: VirtualControlElementIdData::from_model(
                model.control_element_id.get(),
            ),
            fx_snapshot: model.fx_snapshot.get_ref().clone(),
            touched_parameter_type: model.touched_parameter_type.get(),
            bookmark_data: BookmarkData {
                anchor: model.bookmark_anchor_type.get(),
                r#ref: model.bookmark_ref.get(),
                is_region: model.bookmark_type.get() == BookmarkType::Region,
            },
            seek_options: model.seek_options(),
            track_area: model.track_area.get(),
            track_automation_mode: model.automation_mode.get(),
            automation_mode_override_type: model.automation_mode_override_type.get(),
            fx_display_type: model.fx_display_type.get(),
            scroll_arrange_view: model.scroll_arrange_view.get(),
            scroll_mixer: model.scroll_mixer.get(),
            send_midi_destination: model.send_midi_destination.get(),
            raw_midi_pattern: model.raw_midi_pattern.get_ref().clone(),
            osc_address_pattern: model.osc_address_pattern.get_ref().clone(),
            osc_arg_index: model.osc_arg_index.get(),
            osc_arg_type: model.osc_arg_type_tag.get(),
            osc_dev_id: model.osc_dev_id.get(),
            slot_index: model.slot_index.get(),
            next_bar: model.next_bar.get(),
            buffered: model.buffered.get(),
            poll_for_feedback: model.poll_for_feedback.get(),
            tags: model.tags.get_ref().clone(),
            exclusivity: model.exclusivity.get(),
            group_id: conversion_context
                .group_key_by_id(model.group_id.get())
                .unwrap_or_default(),
            active_mappings_only: model.active_mappings_only.get(),
        }
    }

    pub fn apply_to_model(
        &self,
        model: &mut TargetModel,
        compartment: MappingCompartment,
        context: ExtendedProcessorContext,
        conversion_context: &impl DataToModelConversionContext,
    ) {
        self.apply_to_model_flexible(
            model,
            Some(context),
            Some(App::version()),
            true,
            compartment,
            conversion_context,
        );
    }

    /// The context - if available - will be used to resolve some track/FX properties for UI
    /// convenience. The context is necessary if there's the possibility of loading data saved with
    /// ReaLearn < 1.12.0.
    pub fn apply_to_model_flexible(
        &self,
        model: &mut TargetModel,
        context: Option<ExtendedProcessorContext>,
        preset_version: Option<&Version>,
        with_notification: bool,
        compartment: MappingCompartment,
        conversion_context: &impl DataToModelConversionContext,
    ) {
        let final_category = if self.category.is_allowed_in(compartment) {
            self.category
        } else {
            TargetCategory::default_for(compartment)
        };
        model
            .category
            .set_with_optional_notification(final_category, with_notification);
        model
            .unit
            .set_with_optional_notification(self.unit, with_notification);
        model
            .r#type
            .set_with_optional_notification(self.r#type, with_notification);
        let reaper = Reaper::get();
        let action = match self.command_name.as_ref() {
            None => None,
            Some(command_name) => match command_name.parse::<u32>() {
                // Could parse this as command ID integer. This is a built-in action.
                Ok(command_id_int) => match command_id_int.try_into() {
                    Ok(command_id) => Some(reaper.main_section().action_by_command_id(command_id)),
                    Err(_) => {
                        notification::warn(format!("Invalid command ID {}", command_id_int));
                        None
                    }
                },
                // Couldn't parse this as integer. This is a ReaScript or custom action.
                Err(_) => Some(reaper.action_by_command_name(command_name.as_str())),
            },
        };
        model
            .action
            .set_with_optional_notification(action, with_notification);
        let invocation_type = if let Some(invoke_relative) = self.invoke_relative {
            // Very old ReaLearn version
            if invoke_relative {
                ActionInvocationType::Relative
            } else {
                ActionInvocationType::Absolute
            }
        } else {
            self.invocation_type
        };
        model
            .action_invocation_type
            .set_with_optional_notification(invocation_type, with_notification);
        let track_prop_values = deserialize_track(&self.track_data);
        model.set_track_from_prop_values(
            track_prop_values,
            with_notification,
            context.map(|c| c.context()),
        );
        model
            .enable_only_if_track_selected
            .set_with_optional_notification(
                self.enable_only_if_track_is_selected,
                with_notification,
            );
        model
            .with_track
            .set_with_optional_notification(self.with_track, with_notification);
        let virtual_track = model.virtual_track().unwrap_or(VirtualTrack::This);
        let fx_prop_values = deserialize_fx(
            &self.fx_data,
            context.map(|c| (c, compartment, &virtual_track)),
        );
        model.set_fx_from_prop_values(fx_prop_values, with_notification, context, compartment);
        model
            .enable_only_if_fx_has_focus
            .set_with_optional_notification(self.enable_only_if_fx_has_focus, with_notification);
        let route_prop_values = deserialize_track_route(&self.track_route_data);
        model.set_route(route_prop_values, with_notification);
        let fx_param_prop_values = deserialize_fx_parameter(&self.fx_parameter_data);
        model.set_fx_parameter(fx_param_prop_values, with_notification);
        let track_exclusivity = if let Some(select_exclusively) = self.select_exclusively {
            // Should only be set in versions < 2.4.0.
            if select_exclusively {
                TrackExclusivity::ExclusiveWithinProject
            } else {
                TrackExclusivity::NonExclusive
            }
        } else {
            self.track_exclusivity
        };
        model
            .track_exclusivity
            .set_with_optional_notification(track_exclusivity, with_notification);
        let solo_behavior = self.solo_behavior.unwrap_or_else(|| {
            let is_old_preset = preset_version
                .map(|v| v < &Version::new(2, 4, 0))
                .unwrap_or(true);
            if is_old_preset {
                SoloBehavior::IgnoreRouting
            } else {
                SoloBehavior::InPlace
            }
        });
        model
            .solo_behavior
            .set_with_optional_notification(solo_behavior, with_notification);
        model
            .transport_action
            .set_with_optional_notification(self.transport_action, with_notification);
        model
            .any_on_parameter
            .set_with_optional_notification(self.any_on_parameter, with_notification);
        model
            .control_element_type
            .set_with_optional_notification(self.control_element_type, with_notification);
        model.control_element_id.set_with_optional_notification(
            self.control_element_index.to_model(),
            with_notification,
        );
        model
            .fx_snapshot
            .set_with_optional_notification(self.fx_snapshot.clone(), with_notification);
        model
            .touched_parameter_type
            .set_with_optional_notification(self.touched_parameter_type, with_notification);
        let bookmark_type = if self.bookmark_data.is_region {
            BookmarkType::Region
        } else {
            BookmarkType::Marker
        };
        model
            .bookmark_type
            .set_with_optional_notification(bookmark_type, with_notification);
        model
            .bookmark_anchor_type
            .set_with_optional_notification(self.bookmark_data.anchor, with_notification);
        model
            .bookmark_ref
            .set_with_optional_notification(self.bookmark_data.r#ref, with_notification);
        model.set_seek_options(self.seek_options, with_notification);
        model
            .track_area
            .set_with_optional_notification(self.track_area, with_notification);
        model
            .automation_mode
            .set_with_optional_notification(self.track_automation_mode, with_notification);
        model
            .automation_mode_override_type
            .set_with_optional_notification(self.automation_mode_override_type, with_notification);
        model
            .fx_display_type
            .set_with_optional_notification(self.fx_display_type, with_notification);
        model
            .scroll_arrange_view
            .set_with_optional_notification(self.scroll_arrange_view, with_notification);
        let scroll_mixer = if self.category == TargetCategory::Reaper
            && self.r#type == ReaperTargetType::TrackSelection
        {
            let is_old_preset = preset_version
                .map(|v| v < &Version::new(2, 8, 0))
                .unwrap_or(true);
            if is_old_preset {
                true
            } else {
                self.scroll_mixer
            }
        } else {
            self.scroll_mixer
        };
        model
            .scroll_mixer
            .set_with_optional_notification(scroll_mixer, with_notification);
        model
            .send_midi_destination
            .set_with_optional_notification(self.send_midi_destination, with_notification);
        model
            .raw_midi_pattern
            .set_with_optional_notification(self.raw_midi_pattern.clone(), with_notification);
        model
            .osc_address_pattern
            .set_with_optional_notification(self.osc_address_pattern.clone(), with_notification);
        model
            .osc_arg_index
            .set_with_optional_notification(self.osc_arg_index, with_notification);
        model
            .osc_arg_type_tag
            .set_with_optional_notification(self.osc_arg_type, with_notification);
        model
            .osc_dev_id
            .set_with_optional_notification(self.osc_dev_id, with_notification);
        model
            .slot_index
            .set_with_optional_notification(self.slot_index, with_notification);
        model
            .next_bar
            .set_with_optional_notification(self.next_bar, with_notification);
        model
            .buffered
            .set_with_optional_notification(self.buffered, with_notification);
        model
            .poll_for_feedback
            .set_with_optional_notification(self.poll_for_feedback, with_notification);
        model
            .tags
            .set_with_optional_notification(self.tags.clone(), with_notification);
        model
            .exclusivity
            .set_with_optional_notification(self.exclusivity, with_notification);
        let group_id = conversion_context
            .group_id_by_key(&self.group_id)
            .unwrap_or_default();
        model
            .group_id
            .set_with_optional_notification(group_id, with_notification);
        model
            .active_mappings_only
            .set_with_optional_notification(self.active_mappings_only, with_notification);
    }
}

pub fn serialize_track(track: TrackPropValues) -> TrackData {
    use VirtualTrackType::*;
    match track.r#type {
        This => TrackData {
            guid: None,
            name: None,
            index: None,
            expression: None,
        },
        Selected => TrackData {
            guid: Some("selected".to_string()),
            name: None,
            index: None,
            expression: None,
        },
        AllSelected => TrackData {
            guid: Some("selected*".to_string()),
            name: None,
            index: None,
            expression: None,
        },
        Master => TrackData {
            guid: Some("master".to_string()),
            name: None,
            index: None,
            expression: None,
        },
        ByIdOrName => TrackData {
            guid: track.id.map(|id| id.to_string_without_braces()),
            name: Some(track.name),
            index: None,
            expression: None,
        },
        ById => TrackData {
            guid: track.id.map(|id| id.to_string_without_braces()),
            name: None,
            index: None,
            expression: None,
        },
        ByName => TrackData {
            guid: None,
            name: Some(track.name),
            index: None,
            expression: None,
        },
        AllByName => TrackData {
            guid: Some("name*".to_string()),
            name: Some(track.name),
            index: None,
            expression: None,
        },
        ByIndex => TrackData {
            guid: None,
            name: None,
            index: Some(track.index),
            expression: None,
        },
        Dynamic => TrackData {
            guid: None,
            name: None,
            index: None,
            expression: Some(track.expression),
        },
    }
}

pub fn serialize_fx(fx: FxPropValues) -> FxData {
    use VirtualFxType::*;
    match fx.r#type {
        This => FxData {
            anchor: Some(VirtualFxType::This),
            guid: None,
            index: None,
            name: None,
            is_input_fx: false,
            expression: None,
        },
        Focused => FxData {
            anchor: Some(VirtualFxType::Focused),
            guid: None,
            index: None,
            name: None,
            is_input_fx: false,
            expression: None,
        },
        Dynamic => FxData {
            anchor: Some(VirtualFxType::Dynamic),
            guid: None,
            index: None,
            name: None,
            is_input_fx: fx.is_input_fx,
            expression: Some(fx.expression),
        },
        ById => FxData {
            anchor: Some(VirtualFxType::ById),
            index: Some(fx.index),
            guid: fx.id.map(|id| id.to_string_without_braces()),
            name: None,
            is_input_fx: fx.is_input_fx,
            expression: None,
        },
        ByName => FxData {
            anchor: Some(VirtualFxType::ByName),
            index: None,
            guid: None,
            name: Some(fx.name),
            is_input_fx: fx.is_input_fx,
            expression: None,
        },
        AllByName => FxData {
            anchor: Some(VirtualFxType::AllByName),
            index: None,
            guid: None,
            name: Some(fx.name),
            is_input_fx: fx.is_input_fx,
            expression: None,
        },
        ByIndex => FxData {
            anchor: Some(VirtualFxType::ByIndex),
            index: Some(fx.index),
            guid: None,
            name: None,
            is_input_fx: fx.is_input_fx,
            expression: None,
        },
        ByIdOrIndex => FxData {
            anchor: Some(VirtualFxType::ByIdOrIndex),
            index: Some(fx.index),
            guid: fx.id.map(|id| id.to_string_without_braces()),
            name: None,
            is_input_fx: fx.is_input_fx,
            expression: None,
        },
    }
}

pub fn serialize_fx_parameter(param: FxParameterPropValues) -> FxParameterData {
    use VirtualFxParameterType::*;
    match param.r#type {
        Dynamic => FxParameterData {
            r#type: Some(param.r#type),
            index: 0,
            name: None,
            expression: Some(param.expression),
        },
        ByName => FxParameterData {
            r#type: Some(param.r#type),
            index: 0,
            name: Some(param.name),
            expression: None,
        },
        ById => FxParameterData {
            // Before 2.8.0 we didn't have a type and this was the default ... let's leave it
            // at that.
            r#type: None,
            index: param.index,
            name: None,
            expression: None,
        },
        ByIndex => FxParameterData {
            // Before 2.8.0 we didn't have a type and this was the default ... let's leave it
            // at that.
            r#type: Some(param.r#type),
            index: param.index,
            name: None,
            expression: None,
        },
    }
}

pub fn serialize_track_route(route: TrackRoutePropValues) -> TrackRouteData {
    use TrackRouteSelectorType::*;
    match route.selector_type {
        Dynamic => TrackRouteData {
            selector_type: Some(route.selector_type),
            r#type: route.r#type,
            index: None,
            guid: None,
            name: None,
            expression: Some(route.expression),
        },
        ById => TrackRouteData {
            selector_type: Some(route.selector_type),
            r#type: route.r#type,
            index: None,
            guid: route.id.map(|id| id.to_string_without_braces()),
            name: None,
            expression: None,
        },
        ByName => TrackRouteData {
            selector_type: Some(route.selector_type),
            r#type: route.r#type,
            index: None,
            guid: None,
            name: Some(route.name),
            expression: None,
        },
        ByIndex => TrackRouteData {
            // Before 2.8.0 we didn't have a selector type and this was the default ... let's leave
            // it at that.
            selector_type: None,
            r#type: route.r#type,
            index: Some(route.index),
            guid: None,
            name: None,
            expression: None,
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxParameterData {
    #[serde(rename = "paramType", default, skip_serializing_if = "is_default")]
    r#type: Option<VirtualFxParameterType>,
    #[serde(
        rename = "paramIndex",
        deserialize_with = "f32_as_u32",
        default,
        skip_serializing_if = "is_default"
    )]
    index: u32,
    #[serde(rename = "paramName", default, skip_serializing_if = "is_default")]
    name: Option<String>,
    #[serde(
        rename = "paramExpression",
        default,
        skip_serializing_if = "is_default"
    )]
    expression: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackRouteData {
    #[serde(
        rename = "routeSelectorType",
        default,
        skip_serializing_if = "is_default"
    )]
    pub selector_type: Option<TrackRouteSelectorType>,
    #[serde(rename = "routeType", default, skip_serializing_if = "is_default")]
    pub r#type: TrackRouteType,
    /// The only reason this is an option is that in ReaLearn < 1.11.0 we allowed the send
    /// index to be undefined (-1). However, going with a default of 0 is also okay so
    /// `None` and `Some(0)` means essentially the same thing to us now.
    #[serde(
        rename = "sendIndex",
        deserialize_with = "none_if_minus_one",
        default,
        skip_serializing_if = "is_none_or_some_default"
    )]
    pub index: Option<u32>,
    #[serde(rename = "routeGuid", default, skip_serializing_if = "is_default")]
    pub guid: Option<String>,
    #[serde(rename = "routeName", default, skip_serializing_if = "is_default")]
    pub name: Option<String>,
    #[serde(
        rename = "routeExpression",
        default,
        skip_serializing_if = "is_default"
    )]
    pub expression: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxData {
    /// Since 1.12.0-pre8. This is an option because we changed the default and wanted an easy
    /// way to detect when an old preset is loaded.
    // TODO-low If we would have a look at the version number at deserialization time, we could
    //  make it work without the option. Then we could also go without redundant "fxAnchor": "id" in
    //  current JSON. However, we introduced version numbers in 1.12.0-pre18 so this could
    //  negatively effect some prerelease testers. Another way to get rid of the redundant
    //  "fxAnchor" property would be to set this to none if the target type doesn't support FX.
    #[serde(rename = "fxAnchor", default, skip_serializing_if = "is_default")]
    pub anchor: Option<VirtualFxType>,
    /// The only reason this is an option is that in ReaLearn < 1.11.0 we allowed the FX
    /// index to be undefined (-1). However, going with a default of 0 is also okay so
    /// `None` and `Some(0)` means essentially the same thing to us now.
    #[serde(
        rename = "fxIndex",
        deserialize_with = "none_if_minus_one",
        default,
        skip_serializing_if = "is_none_or_some_default"
    )]
    pub index: Option<u32>,
    /// Since 1.12.0-pre1
    #[serde(rename = "fxGUID", default, skip_serializing_if = "is_default")]
    pub guid: Option<String>,
    /// Since 1.12.0-pre8
    #[serde(rename = "fxName", default, skip_serializing_if = "is_default")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub is_input_fx: bool,
    #[serde(rename = "fxExpression", default, skip_serializing_if = "is_default")]
    pub expression: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackData {
    // None means "This" track
    #[serde(rename = "trackGUID", default, skip_serializing_if = "is_default")]
    guid: Option<String>,
    #[serde(rename = "trackName", default, skip_serializing_if = "is_default")]
    name: Option<String>,
    #[serde(rename = "trackIndex", default, skip_serializing_if = "is_default")]
    index: Option<u32>,
    #[serde(
        rename = "trackExpression",
        default,
        skip_serializing_if = "is_default"
    )]
    expression: Option<String>,
}

pub fn deserialize_track(track_data: &TrackData) -> TrackPropValues {
    match track_data {
        TrackData {
            guid: None,
            name: None,
            index: None,
            expression: None,
        } => TrackPropValues::from_virtual_track(VirtualTrack::This),
        TrackData { guid: Some(g), .. } if g == "master" => {
            TrackPropValues::from_virtual_track(VirtualTrack::Master)
        }
        TrackData { guid: Some(g), .. } if g == "selected" => {
            TrackPropValues::from_virtual_track(VirtualTrack::Selected {
                allow_multiple: false,
            })
        }
        TrackData { guid: Some(g), .. } if g == "selected*" => {
            TrackPropValues::from_virtual_track(VirtualTrack::Selected {
                allow_multiple: true,
            })
        }
        TrackData {
            guid: Some(g),
            name: Some(n),
            ..
        } if g == "name*" => TrackPropValues {
            r#type: VirtualTrackType::AllByName,
            name: n.clone(),
            ..Default::default()
        },
        TrackData {
            guid: Some(g),
            name,
            ..
        } => {
            let id = Guid::from_string_without_braces(g).ok();
            match name {
                None => TrackPropValues {
                    r#type: VirtualTrackType::ById,
                    id,
                    ..Default::default()
                },
                Some(n) => TrackPropValues {
                    r#type: VirtualTrackType::ByIdOrName,
                    id,
                    name: n.clone(),
                    ..Default::default()
                },
            }
        }
        TrackData {
            guid: None,
            name: Some(n),
            ..
        } => TrackPropValues {
            r#type: VirtualTrackType::ByName,
            name: n.clone(),
            ..Default::default()
        },
        TrackData {
            guid: None,
            name: None,
            index: Some(i),
            ..
        } => TrackPropValues {
            r#type: VirtualTrackType::ByIndex,
            index: *i,
            ..Default::default()
        },
        TrackData {
            guid: None,
            name: None,
            index: None,
            expression: Some(e),
        } => TrackPropValues {
            r#type: VirtualTrackType::Dynamic,
            expression: e.clone(),
            ..Default::default()
        },
    }
}

/// The context and so on is only necessary if you want to load < 1.12.0 presets.
pub fn deserialize_fx(
    fx_data: &FxData,
    ctx: Option<(ExtendedProcessorContext, MappingCompartment, &VirtualTrack)>,
) -> FxPropValues {
    match fx_data {
        // Special case: <Focused> for ReaLearn < 2.8.0-pre4.
        FxData { guid: Some(g), .. } if g == "focused" => FxPropValues {
            r#type: VirtualFxType::Focused,
            ..Default::default()
        },
        // Before ReaLearn 1.12.0 only the index was saved, even if it was (implicitly) always
        // IdOrIndex anchor. The GUID was looked up at runtime whenever loading the project. Do it!
        FxData {
            anchor: None,
            guid: None,
            expression: None,
            index: Some(i),
            is_input_fx,
            ..
        } => {
            let (context, compartment, virtual_track) =
                ctx.expect("trying to load < 1.12.0 FX target without processor context");
            let fx =
                get_guid_based_fx_at_index(context, virtual_track, *is_input_fx, *i, compartment)
                    .ok();
            FxPropValues {
                r#type: VirtualFxType::ByIdOrIndex,
                is_input_fx: *is_input_fx,
                id: fx.and_then(|f| f.guid()),
                index: *i,
                ..Default::default()
            }
        }
        // In ReaLearn 1.12.0-pre1 we started also saving the GUID, even for IdOrIndex anchor. We
        // still want to support that, even if no anchor is given.
        FxData {
            anchor: None,
            guid: Some(guid_string),
            name: None,
            expression: None,
            index: Some(index),
            is_input_fx,
        } => {
            let id = Guid::from_string_without_braces(guid_string).ok();
            FxPropValues {
                r#type: VirtualFxType::ByIdOrIndex,
                is_input_fx: *is_input_fx,
                id,
                index: *index,
                ..Default::default()
            }
        }
        // Since ReaLearn 1.12.0-pre8 we support Index anchor. We can't distinguish from < 1.12.0
        // data without explicitly given anchor.
        FxData {
            anchor: Some(VirtualFxType::ByIndex),
            guid: None,
            expression: None,
            index: Some(i),
            is_input_fx,
            ..
        } => FxPropValues {
            r#type: VirtualFxType::ByIndex,
            is_input_fx: *is_input_fx,
            index: *i,
            ..Default::default()
        },
        // Since ReaLearn 1.12.0 to 2.8.0-pre2. We try to guess the anchor (what a mess).
        FxData {
            anchor: None,
            guid: Some(guid_string),
            name: _,
            expression: _,
            index,
            is_input_fx,
        } => {
            let id = Guid::from_string_without_braces(guid_string).ok();
            FxPropValues {
                r#type: VirtualFxType::ById,
                is_input_fx: *is_input_fx,
                id,
                index: index.unwrap_or_default(),
                ..Default::default()
            }
        }
        FxData {
            anchor: None,
            index: _,
            guid: _,
            name: Some(name),
            is_input_fx,
            expression: None,
        } => FxPropValues {
            r#type: VirtualFxType::ByName,
            is_input_fx: *is_input_fx,
            name: name.clone(),
            ..Default::default()
        },
        FxData {
            // Here we don't necessarily need the name anchor because there's no ambiguity.
            anchor: None,
            index: _,
            guid: _,
            name: _,
            is_input_fx: _,
            expression: Some(e),
        } => FxPropValues {
            r#type: VirtualFxType::Dynamic,
            expression: e.clone(),
            ..Default::default()
        },
        // >= 2.8.0-pre3. Take everything we can get but watch the anchor.
        FxData {
            anchor: Some(fx_type),
            index,
            guid,
            name,
            is_input_fx,
            expression,
        } => FxPropValues {
            r#type: *fx_type,
            is_input_fx: *is_input_fx,
            id: guid
                .as_ref()
                .and_then(|g| Guid::from_string_without_braces(g).ok()),
            name: name.clone().unwrap_or_default(),
            expression: expression.clone().unwrap_or_default(),
            index: index.unwrap_or_default(),
        },
        FxData {
            anchor: None,
            index: None,
            guid: None,
            name: None,
            expression: None,
            is_input_fx: _,
        } => FxPropValues::default(),
    }
}

pub fn deserialize_fx_parameter(param_data: &FxParameterData) -> FxParameterPropValues {
    match param_data {
        // This is the case for versions < 2.8.0.
        FxParameterData {
            // Important (because index is always given we need this as distinction).
            r#type: None,
            index: i,
            ..
        } => FxParameterPropValues {
            r#type: VirtualFxParameterType::ById,
            index: *i,
            ..Default::default()
        },
        FxParameterData {
            name: Some(name), ..
        } => FxParameterPropValues {
            r#type: VirtualFxParameterType::ByName,
            name: name.clone(),
            ..Default::default()
        },
        FxParameterData {
            expression: Some(e),
            ..
        } => FxParameterPropValues {
            r#type: VirtualFxParameterType::Dynamic,
            expression: e.clone(),
            ..Default::default()
        },
        FxParameterData {
            r#type: Some(VirtualFxParameterType::ByIndex),
            index: i,
            ..
        } => FxParameterPropValues {
            r#type: VirtualFxParameterType::ByIndex,
            index: *i,
            ..Default::default()
        },
        _ => FxParameterPropValues::default(),
    }
}

pub fn deserialize_track_route(data: &TrackRouteData) -> TrackRoutePropValues {
    match data {
        // This is the case for versions < 2.8.0.
        TrackRouteData {
            // Important (because index is always given we need this as distinction).
            selector_type: None,
            r#type: TrackRouteType::Send,
            index: Some(i),
            ..
        } => TrackRoutePropValues {
            selector_type: TrackRouteSelectorType::ByIndex,
            r#type: TrackRouteType::Send,
            index: *i,
            ..Default::default()
        },
        // These are the new ones.
        TrackRouteData {
            selector_type: Some(TrackRouteSelectorType::ById),
            r#type: t,
            guid: Some(g),
            ..
        } => {
            let id = Guid::from_string_without_braces(g).ok();
            TrackRoutePropValues {
                selector_type: TrackRouteSelectorType::ById,
                r#type: *t,
                id,
                ..Default::default()
            }
        }
        TrackRouteData {
            selector_type: Some(TrackRouteSelectorType::ByIndex) | None,
            r#type: t,
            index: i,
            ..
        } => TrackRoutePropValues {
            selector_type: TrackRouteSelectorType::ByIndex,
            r#type: *t,
            index: i.unwrap_or(0),
            ..Default::default()
        },
        TrackRouteData {
            selector_type: Some(TrackRouteSelectorType::ByName),
            r#type: t,
            name: Some(name),
            ..
        } => TrackRoutePropValues {
            selector_type: TrackRouteSelectorType::ByName,
            r#type: *t,
            name: name.clone(),
            ..Default::default()
        },
        TrackRouteData {
            selector_type: Some(TrackRouteSelectorType::Dynamic),
            r#type: t,
            expression: Some(e),
            ..
        } => TrackRoutePropValues {
            selector_type: TrackRouteSelectorType::Dynamic,
            r#type: *t,
            expression: e.clone(),
            ..Default::default()
        },
        _ => TrackRoutePropValues::default(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkData {
    #[serde(rename = "bookmarkAnchor", default, skip_serializing_if = "is_default")]
    pub anchor: BookmarkAnchorType,
    #[serde(rename = "bookmarkRef", default, skip_serializing_if = "is_default")]
    pub r#ref: u32,
    #[serde(
        rename = "bookmarkIsRegion",
        default,
        skip_serializing_if = "is_default"
    )]
    pub is_region: bool,
}

pub fn get_guid_based_fx_at_index(
    context: ExtendedProcessorContext,
    track: &VirtualTrack,
    is_input_fx: bool,
    fx_index: u32,
    compartment: MappingCompartment,
) -> Result<Fx, &'static str> {
    let fx_chain = get_fx_chain(context, track, is_input_fx, compartment)?;
    fx_chain.fx_by_index(fx_index).ok_or("no FX at that index")
}
