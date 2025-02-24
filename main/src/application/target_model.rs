use crate::base::default_util::is_default;
use crate::base::{prop, Prop};
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{ControlType, OscArgDescriptor, OscTypeTag, Target};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{
    Action, BookmarkType, Fx, FxParameter, Guid, Project, Track, TrackRoute, TrackRoutePartner,
};

use rxrust::prelude::*;
use serde::{Deserialize, Serialize};

use crate::application::VirtualControlElementType;
use crate::domain::{
    find_bookmark, get_fx_param, get_fxs, get_non_present_virtual_route_label,
    get_non_present_virtual_track_label, get_track_route, ActionInvocationType, AnyOnParameter,
    CompoundMappingTarget, Exclusivity, ExpressionEvaluator, ExtendedProcessorContext,
    FeedbackResolution, FxDescriptor, FxDisplayType, FxParameterDescriptor, GroupId,
    MappingCompartment, OscDeviceId, ProcessorContext, RealearnTarget, ReaperTarget,
    ReaperTargetType, SeekOptions, SendMidiDestination, SlotPlayOptions, SoloBehavior, Tag,
    TagScope, TouchedParameterType, TrackDescriptor, TrackExclusivity, TrackRouteDescriptor,
    TrackRouteSelector, TrackRouteType, TransportAction, UnresolvedActionTarget,
    UnresolvedAllTrackFxEnableTarget, UnresolvedAnyOnTarget,
    UnresolvedAutomationModeOverrideTarget, UnresolvedAutomationTouchStateTarget,
    UnresolvedClipSeekTarget, UnresolvedClipTransportTarget, UnresolvedClipVolumeTarget,
    UnresolvedCompoundMappingTarget, UnresolvedEnableInstancesTarget,
    UnresolvedEnableMappingsTarget, UnresolvedFxEnableTarget, UnresolvedFxNavigateTarget,
    UnresolvedFxOpenTarget, UnresolvedFxParameterTarget, UnresolvedFxPresetTarget,
    UnresolvedGoToBookmarkTarget, UnresolvedLastTouchedTarget, UnresolvedLoadFxSnapshotTarget,
    UnresolvedLoadMappingSnapshotTarget, UnresolvedMidiSendTarget,
    UnresolvedNavigateWithinGroupTarget, UnresolvedOscSendTarget, UnresolvedPlayrateTarget,
    UnresolvedReaperTarget, UnresolvedRouteAutomationModeTarget, UnresolvedRouteMonoTarget,
    UnresolvedRouteMuteTarget, UnresolvedRoutePanTarget, UnresolvedRoutePhaseTarget,
    UnresolvedRouteVolumeTarget, UnresolvedSeekTarget, UnresolvedSelectedTrackTarget,
    UnresolvedTempoTarget, UnresolvedTrackArmTarget, UnresolvedTrackAutomationModeTarget,
    UnresolvedTrackMuteTarget, UnresolvedTrackPanTarget, UnresolvedTrackPeakTarget,
    UnresolvedTrackPhaseTarget, UnresolvedTrackSelectionTarget, UnresolvedTrackShowTarget,
    UnresolvedTrackSoloTarget, UnresolvedTrackToolTarget, UnresolvedTrackVolumeTarget,
    UnresolvedTrackWidthTarget, UnresolvedTransportTarget, VirtualChainFx, VirtualControlElement,
    VirtualControlElementId, VirtualFx, VirtualFxParameter, VirtualTarget, VirtualTrack,
    VirtualTrackRoute,
};
use serde_repr::*;
use std::borrow::Cow;
use std::error::Error;

use reaper_medium::{
    AutomationMode, BookmarkId, GlobalAutomationModeOverride, TrackArea, TrackLocation,
    TrackSendDirection,
};
use std::fmt;
use std::fmt::{Display, Formatter};
use std::rc::Rc;
use wildmatch::WildMatch;

/// A model for creating targets
#[derive(Clone, Debug)]
pub struct TargetModel {
    // # For all targets
    pub category: Prop<TargetCategory>,
    pub unit: Prop<TargetUnit>,
    // # For virtual targets
    pub control_element_type: Prop<VirtualControlElementType>,
    pub control_element_id: Prop<VirtualControlElementId>,
    // # For REAPER targets
    // TODO-low Rename this to reaper_target_type
    pub r#type: Prop<ReaperTargetType>,
    // # For action targets only
    // TODO-low Maybe replace Action with just command ID and/or command name
    pub action: Prop<Option<Action>>,
    pub action_invocation_type: Prop<ActionInvocationType>,
    pub with_track: Prop<bool>,
    // # For track targets
    pub track_type: Prop<VirtualTrackType>,
    pub track_id: Prop<Option<Guid>>,
    pub track_name: Prop<String>,
    pub track_index: Prop<u32>,
    pub track_expression: Prop<String>,
    pub enable_only_if_track_selected: Prop<bool>,
    // # For track FX targets
    pub fx_type: Prop<VirtualFxType>,
    pub fx_is_input_fx: Prop<bool>,
    pub fx_id: Prop<Option<Guid>>,
    pub fx_name: Prop<String>,
    pub fx_index: Prop<u32>,
    pub fx_expression: Prop<String>,
    pub enable_only_if_fx_has_focus: Prop<bool>,
    // # For track FX parameter targets
    pub param_type: Prop<VirtualFxParameterType>,
    pub param_index: Prop<u32>,
    pub param_name: Prop<String>,
    pub param_expression: Prop<String>,
    // # For track route targets
    pub route_selector_type: Prop<TrackRouteSelectorType>,
    pub route_type: Prop<TrackRouteType>,
    pub route_id: Prop<Option<Guid>>,
    pub route_index: Prop<u32>,
    pub route_name: Prop<String>,
    pub route_expression: Prop<String>,
    // # For track solo targets
    pub solo_behavior: Prop<SoloBehavior>,
    // # For toggleable track targets
    pub track_exclusivity: Prop<TrackExclusivity>,
    // # For transport target
    pub transport_action: Prop<TransportAction>,
    // # For any-on target
    pub any_on_parameter: Prop<AnyOnParameter>,
    // # For "Load FX snapshot" target
    pub fx_snapshot: Prop<Option<FxSnapshot>>,
    // # For "Automation touch state" target
    pub touched_parameter_type: Prop<TouchedParameterType>,
    // # For "Go to marker/region" target
    pub bookmark_ref: Prop<u32>,
    pub bookmark_type: Prop<BookmarkType>,
    pub bookmark_anchor_type: Prop<BookmarkAnchorType>,
    // # For "Go to marker/region" target and "Seek" target
    pub use_time_selection: Prop<bool>,
    pub use_loop_points: Prop<bool>,
    // # For "Seek" target
    pub use_regions: Prop<bool>,
    pub use_project: Prop<bool>,
    pub move_view: Prop<bool>,
    pub seek_play: Prop<bool>,
    pub feedback_resolution: Prop<FeedbackResolution>,
    // # For track show target
    pub track_area: Prop<RealearnTrackArea>,
    // # For track and route automation mode target
    pub automation_mode: Prop<RealearnAutomationMode>,
    // # For automation mode override target
    pub automation_mode_override_type: Prop<AutomationModeOverrideType>,
    // # For FX Open and FX Navigate target
    pub fx_display_type: Prop<FxDisplayType>,
    // # For track selection related targets
    pub scroll_arrange_view: Prop<bool>,
    pub scroll_mixer: Prop<bool>,
    // # For Send MIDI target
    pub raw_midi_pattern: Prop<String>,
    pub send_midi_destination: Prop<SendMidiDestination>,
    // # For Send OSC target
    pub osc_address_pattern: Prop<String>,
    pub osc_arg_index: Prop<Option<u32>>,
    pub osc_arg_type_tag: Prop<OscTypeTag>,
    pub osc_dev_id: Prop<Option<OscDeviceId>>,
    // # For clip targets
    pub slot_index: Prop<usize>,
    pub next_bar: Prop<bool>,
    pub buffered: Prop<bool>,
    // # For targets that might have to be polled in order to get automatic feedback in all cases.
    pub poll_for_feedback: Prop<bool>,
    pub tags: Prop<Vec<Tag>>,
    pub exclusivity: Prop<Exclusivity>,
    pub group_id: Prop<GroupId>,
    pub active_mappings_only: Prop<bool>,
}

impl Default for TargetModel {
    fn default() -> Self {
        Self {
            category: prop(TargetCategory::default()),
            unit: prop(Default::default()),
            control_element_type: prop(VirtualControlElementType::default()),
            control_element_id: prop(Default::default()),
            r#type: prop(ReaperTargetType::FxParameter),
            action: prop(None),
            action_invocation_type: prop(ActionInvocationType::default()),
            track_type: prop(Default::default()),
            track_id: prop(None),
            track_name: prop("".to_owned()),
            track_index: prop(0),
            track_expression: prop("".to_owned()),
            enable_only_if_track_selected: prop(false),
            with_track: prop(false),
            fx_type: prop(Default::default()),
            fx_is_input_fx: prop(false),
            fx_id: prop(None),
            fx_name: prop("".to_owned()),
            fx_index: prop(0),
            fx_expression: prop("".to_owned()),
            enable_only_if_fx_has_focus: prop(false),
            param_type: prop(Default::default()),
            param_index: prop(0),
            param_name: prop("".to_owned()),
            param_expression: prop("".to_owned()),
            route_selector_type: prop(Default::default()),
            route_type: prop(Default::default()),
            route_id: prop(None),
            route_index: prop(0),
            route_name: prop(Default::default()),
            route_expression: prop(Default::default()),
            solo_behavior: prop(Default::default()),
            track_exclusivity: prop(Default::default()),
            transport_action: prop(TransportAction::default()),
            any_on_parameter: prop(AnyOnParameter::default()),
            fx_snapshot: prop(None),
            touched_parameter_type: prop(Default::default()),
            bookmark_ref: prop(0),
            bookmark_type: prop(BookmarkType::Marker),
            bookmark_anchor_type: prop(Default::default()),
            use_time_selection: prop(false),
            use_loop_points: prop(false),
            use_regions: prop(false),
            use_project: prop(true),
            move_view: prop(true),
            seek_play: prop(true),
            feedback_resolution: prop(Default::default()),
            track_area: prop(Default::default()),
            automation_mode: prop(Default::default()),
            automation_mode_override_type: prop(Default::default()),
            fx_display_type: prop(Default::default()),
            scroll_arrange_view: prop(false),
            scroll_mixer: prop(false),
            raw_midi_pattern: prop(Default::default()),
            send_midi_destination: prop(Default::default()),
            osc_address_pattern: prop("".to_owned()),
            osc_arg_index: prop(Some(0)),
            osc_arg_type_tag: prop(Default::default()),
            osc_dev_id: prop(None),
            slot_index: prop(0),
            next_bar: prop(false),
            buffered: prop(false),
            poll_for_feedback: prop(true),
            tags: prop(Default::default()),
            exclusivity: prop(Default::default()),
            group_id: prop(Default::default()),
            active_mappings_only: prop(false),
        }
    }
}

impl TargetModel {
    pub fn supports_control(&self) -> bool {
        use TargetCategory::*;
        match self.category.get() {
            Reaper => self.r#type.get().supports_control(),
            Virtual => true,
        }
    }

    pub fn supports_feedback(&self) -> bool {
        use TargetCategory::*;
        match self.category.get() {
            Reaper => self.r#type.get().supports_feedback(),
            Virtual => true,
        }
    }

    pub fn make_track_sticky(
        &mut self,
        compartment: MappingCompartment,
        context: ExtendedProcessorContext,
    ) -> Result<(), Box<dyn Error>> {
        if self.track_type.get().is_sticky() {
            return Ok(());
        };
        let track = self
            .with_context(context, compartment)
            .first_effective_track()?;
        let virtual_track = virtualize_track(&track, context.context(), false);
        self.set_virtual_track(virtual_track, Some(context.context()));
        Ok(())
    }

    pub fn make_fx_sticky(
        &mut self,
        compartment: MappingCompartment,
        context: ExtendedProcessorContext,
    ) -> Result<(), Box<dyn Error>> {
        if self.fx_type.get().is_sticky() {
            return Ok(());
        };
        let fx = self.with_context(context, compartment).first_fx()?;
        let virtual_fx = virtualize_fx(&fx, context.context(), false);
        self.set_virtual_fx(virtual_fx, context, compartment);
        Ok(())
    }

    pub fn make_route_sticky(
        &mut self,
        compartment: MappingCompartment,
        context: ExtendedProcessorContext,
    ) -> Result<(), Box<dyn Error>> {
        if self.route_selector_type.get().is_sticky() {
            return Ok(());
        };
        let desc = self.track_route_descriptor()?;
        let route = desc.resolve_first(context, compartment)?;
        let virtual_route = virtualize_route(&route, context.context(), false);
        self.set_virtual_route(virtual_route);
        Ok(())
    }

    pub fn take_fx_snapshot(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<FxSnapshot, &'static str> {
        let fx = self.with_context(context, compartment).first_fx()?;
        let fx_info = fx.info()?;
        let fx_snapshot = FxSnapshot {
            fx_type: if fx_info.sub_type_expression.is_empty() {
                fx_info.type_expression
            } else {
                fx_info.sub_type_expression
            },
            fx_name: fx_info.effect_name,
            preset_name: fx.preset_name().map(|n| n.into_string()),
            chunk: Rc::new(fx.tag_chunk()?.content().to_owned()),
        };
        Ok(fx_snapshot)
    }

    pub fn invalidate_fx_index(
        &mut self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) {
        if !self.supports_fx() {
            return;
        }
        if let Ok(actual_fx) = self.with_context(context, compartment).first_fx() {
            let new_virtual_fx = match self.virtual_fx() {
                Some(virtual_fx) => {
                    match virtual_fx {
                        VirtualFx::ChainFx {
                            is_input_fx,
                            chain_fx: anchor,
                        } => match anchor {
                            VirtualChainFx::ByIdOrIndex(guid, _) => Some(VirtualFx::ChainFx {
                                is_input_fx,
                                chain_fx: VirtualChainFx::ByIdOrIndex(guid, actual_fx.index()),
                            }),
                            _ => None,
                        },
                        // No update necessary
                        VirtualFx::Focused | VirtualFx::This => None,
                    }
                }
                // Shouldn't happen
                None => None,
            };
            if let Some(virtual_fx) = new_virtual_fx {
                self.set_virtual_fx(virtual_fx, context, compartment);
            }
        }
    }

    pub fn set_virtual_track(&mut self, track: VirtualTrack, context: Option<&ProcessorContext>) {
        self.set_track_from_prop_values(TrackPropValues::from_virtual_track(track), true, context);
    }

    pub fn set_track_type_from_ui(
        &mut self,
        track_type: VirtualTrackType,
        context: &ProcessorContext,
        initiator: Option<u32>,
    ) {
        use VirtualTrackType::*;
        match track_type {
            This => self.set_concrete_track(
                ConcreteTrackInstruction::This(Some(context)),
                true,
                false,
                initiator,
            ),
            ById => self.set_concrete_track(
                ConcreteTrackInstruction::ById {
                    id: None,
                    context: Some(context),
                },
                true,
                false,
                initiator,
            ),
            _ => self.track_type.set(track_type),
        }
    }

    pub fn set_fx_type_from_ui(
        &mut self,
        fx_type: VirtualFxType,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) {
        use VirtualFxType::*;
        match fx_type {
            This => self.set_concrete_fx(
                ConcreteFxInstruction::This(Some(context.context())),
                true,
                false,
            ),
            ById => self.set_concrete_fx(
                ConcreteFxInstruction::ById {
                    is_input_fx: None,
                    id: None,
                    track: self
                        .with_context(context, compartment)
                        .first_effective_track()
                        .ok(),
                },
                true,
                false,
            ),
            _ => self.fx_type.set(fx_type),
        }
    }

    pub fn set_track_from_prop_values(
        &mut self,
        track: TrackPropValues,
        with_notification: bool,
        context: Option<&ProcessorContext>,
    ) {
        self.track_type
            .set_with_optional_notification(track.r#type, with_notification);
        self.track_expression
            .set_with_optional_notification(track.expression, with_notification);
        use VirtualTrackType::*;
        match track.r#type {
            This => self.set_concrete_track(
                ConcreteTrackInstruction::This(context),
                // Already notified above
                false,
                with_notification,
                None,
            ),
            ById => self.set_concrete_track(
                ConcreteTrackInstruction::ById {
                    id: track.id,
                    context,
                },
                // Already notified above
                false,
                with_notification,
                None,
            ),
            ByName | AllByName => {
                self.track_name
                    .set_with_optional_notification(track.name, with_notification);
            }
            ByIndex => {
                self.track_index
                    .set_with_optional_notification(track.index, with_notification);
            }
            ByIdOrName => {
                self.track_id
                    .set_with_optional_notification(track.id, with_notification);
                self.track_name
                    .set_with_optional_notification(track.name, with_notification);
            }
            Selected | AllSelected | Dynamic | Master => {}
        }
    }

    pub fn set_virtual_route(&mut self, route: VirtualTrackRoute) {
        self.set_route(TrackRoutePropValues::from_virtual_route(route), true);
    }

    pub fn set_route(&mut self, route: TrackRoutePropValues, with_notification: bool) {
        self.route_selector_type
            .set_with_optional_notification(route.selector_type, with_notification);
        self.route_type
            .set_with_optional_notification(route.r#type, with_notification);
        self.route_id
            .set_with_optional_notification(route.id, with_notification);
        self.route_name
            .set_with_optional_notification(route.name, with_notification);
        self.route_index
            .set_with_optional_notification(route.index, with_notification);
        self.route_expression
            .set_with_optional_notification(route.expression, with_notification);
    }

    pub fn set_virtual_fx(
        &mut self,
        fx: VirtualFx,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) {
        self.set_fx_from_prop_values(
            FxPropValues::from_virtual_fx(fx),
            true,
            Some(context),
            compartment,
        );
    }

    pub fn set_fx_from_prop_values(
        &mut self,
        fx: FxPropValues,
        with_notification: bool,
        context: Option<ExtendedProcessorContext>,
        compartment: MappingCompartment,
    ) {
        self.fx_type
            .set_with_optional_notification(fx.r#type, with_notification);
        self.fx_expression
            .set_with_optional_notification(fx.expression, with_notification);
        self.fx_is_input_fx
            .set_with_optional_notification(fx.is_input_fx, with_notification);
        use VirtualFxType::*;
        match fx.r#type {
            This => self.set_concrete_fx(
                ConcreteFxInstruction::This(context.map(|c| c.context())),
                // Already notified above
                false,
                with_notification,
            ),
            ById => self.set_concrete_fx(
                ConcreteFxInstruction::ById {
                    is_input_fx: Some(fx.is_input_fx),
                    id: fx.id,
                    track: context.and_then(|c| {
                        self.with_context(c, compartment)
                            .first_effective_track()
                            .ok()
                    }),
                },
                // Already notified above
                false,
                with_notification,
            ),
            ByName | AllByName => {
                self.fx_name
                    .set_with_optional_notification(fx.name, with_notification);
            }
            ByIndex => {
                self.fx_index
                    .set_with_optional_notification(fx.index, with_notification);
            }
            ByIdOrIndex => {
                self.fx_id
                    .set_with_optional_notification(fx.id, with_notification);
                self.fx_index
                    .set_with_optional_notification(fx.index, with_notification);
            }
            Dynamic | Focused => {}
        }
    }

    pub fn set_fx_parameter(&mut self, param: FxParameterPropValues, with_notification: bool) {
        self.param_type
            .set_with_optional_notification(param.r#type, with_notification);
        self.param_name
            .set_with_optional_notification(param.name, with_notification);
        self.param_index
            .set_with_optional_notification(param.index, with_notification);
        self.param_expression
            .set_with_optional_notification(param.expression, with_notification);
    }

    pub fn set_seek_options(&mut self, options: SeekOptions, with_notification: bool) {
        self.use_time_selection
            .set_with_optional_notification(options.use_time_selection, with_notification);
        self.use_loop_points
            .set_with_optional_notification(options.use_loop_points, with_notification);
        self.use_regions
            .set_with_optional_notification(options.use_regions, with_notification);
        self.use_project
            .set_with_optional_notification(options.use_project, with_notification);
        self.move_view
            .set_with_optional_notification(options.move_view, with_notification);
        self.seek_play
            .set_with_optional_notification(options.seek_play, with_notification);
        self.feedback_resolution
            .set_with_optional_notification(options.feedback_resolution, with_notification);
    }

    /// Sets the track to one of the concrete types ById or This, also setting other important
    /// properties for UI convenience.
    pub fn set_concrete_track(
        &mut self,
        instruction: ConcreteTrackInstruction,
        notify_about_type_change: bool,
        notify_about_id_change: bool,
        initiator: Option<u32>,
    ) {
        let resolved = instruction.resolve();
        self.track_type
            .set_with_optional_notification_and_initiator(
                resolved.virtual_track_type(),
                notify_about_type_change,
                initiator,
            );
        if let Some(id) = resolved.id() {
            self.track_id.set_with_optional_notification_and_initiator(
                Some(id),
                notify_about_id_change,
                initiator,
            );
        }
        // We also set index and name so that we can easily switch between types.
        if let Some(i) = resolved.index() {
            self.track_index.set_without_notification(i);
        }
        if let Some(name) = resolved.name() {
            self.track_name.set_without_notification(name);
        }
    }

    /// Sets the FX to one of the concrete types (ById only for now), also setting other important
    /// properties for UI convenience.
    pub fn set_concrete_fx(
        &mut self,
        instruction: ConcreteFxInstruction,
        notify_about_type_and_input_fx_change: bool,
        notify_about_id_change: bool,
    ) {
        let resolved = instruction.resolve();
        self.fx_type.set_with_optional_notification(
            resolved.virtual_fx_type(),
            notify_about_type_and_input_fx_change,
        );
        if let Some(is_input_fx) = resolved.is_input_fx() {
            self.fx_is_input_fx
                .set_with_optional_notification(is_input_fx, notify_about_type_and_input_fx_change);
        }
        if let Some(id) = resolved.id() {
            self.fx_id
                .set_with_optional_notification(Some(id), notify_about_id_change);
        }
        // We also set index and name so that we can easily switch between types.
        if let Some(i) = resolved.index() {
            self.fx_index.set_without_notification(i);
        }
        if let Some(name) = resolved.name() {
            self.fx_name.set_without_notification(name);
        }
    }

    pub fn seek_options(&self) -> SeekOptions {
        SeekOptions {
            use_time_selection: self.use_time_selection.get(),
            use_loop_points: self.use_loop_points.get(),
            use_regions: self.use_regions.get(),
            use_project: self.use_project.get(),
            move_view: self.move_view.get(),
            seek_play: self.seek_play.get(),
            feedback_resolution: self.feedback_resolution.get(),
        }
    }

    pub fn apply_from_target(
        &mut self,
        target: &ReaperTarget,
        extended_context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) {
        let context = extended_context.context();
        use ReaperTarget::*;
        self.category.set(TargetCategory::Reaper);
        self.r#type.set(ReaperTargetType::from_target(target));
        if let Some(actual_fx) = target.fx() {
            let virtual_fx = virtualize_fx(actual_fx, context, true);
            self.set_virtual_fx(virtual_fx, extended_context, compartment);
            let track = if let Some(track) = actual_fx.track() {
                track.clone()
            } else {
                // Must be monitoring FX. In this case we want the master track (it's REAPER's
                // convention and ours).
                context.project_or_current_project().master_track()
            };
            self.set_virtual_track(virtualize_track(&track, context, true), Some(context));
        } else if let Some(track) = target.track() {
            self.set_virtual_track(virtualize_track(track, context, true), Some(context));
        }
        if let Some(send) = target.route() {
            let virtual_route = virtualize_route(send, context, true);
            self.set_virtual_route(virtual_route);
        }
        if let Some(track_exclusivity) = target.track_exclusivity() {
            self.track_exclusivity.set(track_exclusivity);
        }
        match target {
            Action(t) => {
                self.action.set(Some(t.action.clone()));
                self.action_invocation_type.set(t.invocation_type);
            }
            FxParameter(t) => {
                self.param_type.set(VirtualFxParameterType::ById);
                self.param_index.set(t.param.index());
            }
            Transport(t) => {
                self.transport_action.set(t.action);
            }
            TrackSolo(t) => {
                self.solo_behavior.set(t.behavior);
            }
            GoToBookmark(t) => {
                self.bookmark_ref.set(t.index);
                self.bookmark_type.set(t.bookmark_type);
            }
            TrackAutomationMode(t) => {
                self.automation_mode
                    .set(RealearnAutomationMode::from_reaper(t.mode));
            }
            TrackRouteAutomationMode(t) => {
                self.automation_mode
                    .set(RealearnAutomationMode::from_reaper(t.mode));
            }
            AutomationModeOverride(t) => match t.mode_override {
                None => {
                    self.automation_mode_override_type
                        .set(AutomationModeOverrideType::None);
                }
                Some(GlobalAutomationModeOverride::Bypass) => {
                    self.automation_mode_override_type
                        .set(AutomationModeOverrideType::Bypass);
                }
                Some(GlobalAutomationModeOverride::Mode(am)) => {
                    self.automation_mode_override_type
                        .set(AutomationModeOverrideType::Override);
                    self.automation_mode
                        .set(RealearnAutomationMode::from_reaper(am));
                }
            },
            _ => {}
        };
    }

    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.category
            .changed()
            .merge(self.unit.changed())
            .merge(self.r#type.changed())
            .merge(self.action.changed())
            .merge(self.action_invocation_type.changed())
            .merge(self.track_type.changed())
            .merge(self.track_id.changed())
            .merge(self.track_name.changed())
            .merge(self.track_index.changed())
            .merge(self.track_expression.changed())
            .merge(self.enable_only_if_track_selected.changed())
            .merge(self.with_track.changed())
            .merge(self.fx_type.changed())
            .merge(self.fx_id.changed())
            .merge(self.fx_name.changed())
            .merge(self.fx_index.changed())
            .merge(self.fx_expression.changed())
            .merge(self.fx_is_input_fx.changed())
            .merge(self.enable_only_if_fx_has_focus.changed())
            .merge(self.param_type.changed())
            .merge(self.param_index.changed())
            .merge(self.param_name.changed())
            .merge(self.param_expression.changed())
            .merge(self.route_selector_type.changed())
            .merge(self.route_type.changed())
            .merge(self.route_id.changed())
            .merge(self.route_index.changed())
            .merge(self.route_name.changed())
            .merge(self.route_expression.changed())
            .merge(self.solo_behavior.changed())
            .merge(self.track_exclusivity.changed())
            .merge(self.transport_action.changed())
            .merge(self.any_on_parameter.changed())
            .merge(self.control_element_type.changed())
            .merge(self.control_element_id.changed())
            .merge(self.fx_snapshot.changed())
            .merge(self.touched_parameter_type.changed())
            .merge(self.bookmark_ref.changed())
            .merge(self.bookmark_type.changed())
            .merge(self.bookmark_anchor_type.changed())
            .merge(self.use_time_selection.changed())
            .merge(self.use_loop_points.changed())
            .merge(self.use_regions.changed())
            .merge(self.use_project.changed())
            .merge(self.move_view.changed())
            .merge(self.seek_play.changed())
            .merge(self.feedback_resolution.changed())
            .merge(self.track_area.changed())
            .merge(self.automation_mode.changed())
            .merge(self.automation_mode_override_type.changed())
            .merge(self.fx_display_type.changed())
            .merge(self.scroll_arrange_view.changed())
            .merge(self.scroll_mixer.changed())
            .merge(self.raw_midi_pattern.changed())
            .merge(self.send_midi_destination.changed())
            .merge(self.osc_address_pattern.changed())
            .merge(self.osc_arg_index.changed())
            .merge(self.osc_arg_type_tag.changed())
            .merge(self.osc_dev_id.changed())
            .merge(self.slot_index.changed())
            .merge(self.next_bar.changed())
            .merge(self.buffered.changed())
            .merge(self.poll_for_feedback.changed())
            .merge(self.tags.changed())
            .merge(self.exclusivity.changed())
            .merge(self.group_id.changed())
            .merge(self.active_mappings_only.changed())
    }

    pub fn virtual_track(&self) -> Option<VirtualTrack> {
        use VirtualTrackType::*;
        let track = match self.track_type.get() {
            This => VirtualTrack::This,
            Selected => VirtualTrack::Selected {
                allow_multiple: false,
            },
            AllSelected => VirtualTrack::Selected {
                allow_multiple: true,
            },
            Master => VirtualTrack::Master,
            ById => VirtualTrack::ById(self.track_id.get()?),
            ByName => VirtualTrack::ByName {
                wild_match: WildMatch::new(self.track_name.get_ref()),
                allow_multiple: false,
            },
            AllByName => VirtualTrack::ByName {
                wild_match: WildMatch::new(self.track_name.get_ref()),
                allow_multiple: true,
            },
            ByIndex => VirtualTrack::ByIndex(self.track_index.get()),
            ByIdOrName => VirtualTrack::ByIdOrName(
                self.track_id.get()?,
                WildMatch::new(self.track_name.get_ref()),
            ),
            Dynamic => {
                let evaluator =
                    ExpressionEvaluator::compile(self.track_expression.get_ref()).ok()?;
                VirtualTrack::Dynamic(Box::new(evaluator))
            }
        };
        Some(track)
    }

    pub fn track(&self) -> TrackPropValues {
        TrackPropValues {
            r#type: self.track_type.get(),
            id: self.track_id.get(),
            name: self.track_name.get_ref().clone(),
            expression: self.track_expression.get_ref().clone(),
            index: self.track_index.get(),
        }
    }

    pub fn virtual_fx(&self) -> Option<VirtualFx> {
        use VirtualFxType::*;
        let fx = match self.fx_type.get() {
            Focused => VirtualFx::Focused,
            This => VirtualFx::This,
            _ => VirtualFx::ChainFx {
                is_input_fx: self.fx_is_input_fx.get(),
                chain_fx: self.virtual_chain_fx()?,
            },
        };
        Some(fx)
    }

    pub fn track_route_selector(&self) -> Option<TrackRouteSelector> {
        use TrackRouteSelectorType::*;
        let selector = match self.route_selector_type.get() {
            Dynamic => {
                let evaluator =
                    ExpressionEvaluator::compile(self.route_expression.get_ref()).ok()?;
                TrackRouteSelector::Dynamic(Box::new(evaluator))
            }
            ById => {
                if self.route_type.get() == TrackRouteType::HardwareOutput {
                    // Hardware outputs don't offer stable IDs.
                    TrackRouteSelector::ByIndex(self.route_index.get())
                } else {
                    TrackRouteSelector::ById(self.route_id.get()?)
                }
            }
            ByName => TrackRouteSelector::ByName(WildMatch::new(self.route_name.get_ref())),
            ByIndex => TrackRouteSelector::ByIndex(self.route_index.get()),
        };
        Some(selector)
    }

    pub fn virtual_chain_fx(&self) -> Option<VirtualChainFx> {
        use VirtualFxType::*;
        let fx = match self.fx_type.get() {
            Focused | This => return None,
            ById => VirtualChainFx::ById(self.fx_id.get()?, Some(self.fx_index.get())),
            ByName => VirtualChainFx::ByName {
                wild_match: WildMatch::new(self.fx_name.get_ref()),
                allow_multiple: false,
            },
            AllByName => VirtualChainFx::ByName {
                wild_match: WildMatch::new(self.fx_name.get_ref()),
                allow_multiple: true,
            },
            ByIndex => VirtualChainFx::ByIndex(self.fx_index.get()),
            ByIdOrIndex => VirtualChainFx::ByIdOrIndex(self.fx_id.get(), self.fx_index.get()),
            Dynamic => {
                let evaluator = ExpressionEvaluator::compile(self.fx_expression.get_ref()).ok()?;
                VirtualChainFx::Dynamic(Box::new(evaluator))
            }
        };
        Some(fx)
    }

    pub fn fx(&self) -> FxPropValues {
        FxPropValues {
            r#type: self.fx_type.get(),
            is_input_fx: self.fx_is_input_fx.get(),
            id: self.fx_id.get(),
            name: self.fx_name.get_ref().clone(),
            expression: self.fx_expression.get_ref().clone(),
            index: self.fx_index.get(),
        }
    }

    pub fn track_route(&self) -> TrackRoutePropValues {
        TrackRoutePropValues {
            selector_type: self.route_selector_type.get(),
            r#type: self.route_type.get(),
            id: self.route_id.get(),
            name: self.route_name.get_ref().clone(),
            expression: self.route_expression.get_ref().clone(),
            index: self.route_index.get(),
        }
    }

    pub fn fx_parameter(&self) -> FxParameterPropValues {
        FxParameterPropValues {
            r#type: self.param_type.get(),
            name: self.param_name.get_ref().clone(),
            expression: self.param_expression.get_ref().clone(),
            index: self.param_index.get(),
        }
    }

    pub fn track_descriptor(&self) -> Result<TrackDescriptor, &'static str> {
        let desc = TrackDescriptor {
            track: self.virtual_track().ok_or("virtual track not complete")?,
            enable_only_if_track_selected: self.enable_only_if_track_selected.get(),
        };
        Ok(desc)
    }

    pub fn fx_descriptor(&self) -> Result<FxDescriptor, &'static str> {
        let desc = FxDescriptor {
            track_descriptor: self.track_descriptor()?,
            enable_only_if_fx_has_focus: self.enable_only_if_fx_has_focus.get(),
            fx: self.virtual_fx().ok_or("FX not set")?,
        };
        Ok(desc)
    }

    pub fn track_route_descriptor(&self) -> Result<TrackRouteDescriptor, &'static str> {
        let desc = TrackRouteDescriptor {
            track_descriptor: self.track_descriptor()?,
            route: self.virtual_track_route()?,
        };
        Ok(desc)
    }

    pub fn virtual_track_route(&self) -> Result<VirtualTrackRoute, &'static str> {
        let route = VirtualTrackRoute {
            r#type: self.route_type.get(),
            selector: self.track_route_selector().ok_or("track route not set")?,
        };
        Ok(route)
    }

    pub fn virtual_fx_parameter(&self) -> Option<VirtualFxParameter> {
        use VirtualFxParameterType::*;
        let param = match self.param_type.get() {
            ByName => VirtualFxParameter::ByName(WildMatch::new(self.param_name.get_ref())),
            ById => VirtualFxParameter::ById(self.param_index.get()),
            ByIndex => VirtualFxParameter::ByIndex(self.param_index.get()),
            Dynamic => {
                let evaluator =
                    ExpressionEvaluator::compile(self.param_expression.get_ref()).ok()?;
                VirtualFxParameter::Dynamic(Box::new(evaluator))
            }
        };
        Some(param)
    }

    fn fx_parameter_descriptor(&self) -> Result<FxParameterDescriptor, &'static str> {
        let desc = FxParameterDescriptor {
            fx_descriptor: self.fx_descriptor()?,
            fx_parameter: self.virtual_fx_parameter().ok_or("FX parameter not set")?,
        };
        Ok(desc)
    }

    pub fn create_target(
        &self,
        compartment: MappingCompartment,
    ) -> Result<UnresolvedCompoundMappingTarget, &'static str> {
        use TargetCategory::*;
        match self.category.get() {
            Reaper => {
                use ReaperTargetType::*;
                let target = match self.r#type.get() {
                    Action => UnresolvedReaperTarget::Action(UnresolvedActionTarget {
                        action: self.action()?,
                        invocation_type: self.action_invocation_type.get(),
                        track_descriptor: if self.with_track.get() {
                            Some(self.track_descriptor()?)
                        } else {
                            None
                        },
                    }),
                    FxParameter => {
                        UnresolvedReaperTarget::FxParameter(UnresolvedFxParameterTarget {
                            fx_parameter_descriptor: self.fx_parameter_descriptor()?,
                            poll_for_feedback: self.poll_for_feedback.get(),
                        })
                    }
                    TrackVolume => {
                        UnresolvedReaperTarget::TrackVolume(UnresolvedTrackVolumeTarget {
                            track_descriptor: self.track_descriptor()?,
                        })
                    }
                    TrackTool => UnresolvedReaperTarget::TrackTool(UnresolvedTrackToolTarget {
                        track_descriptor: self.track_descriptor()?,
                    }),
                    TrackPeak => UnresolvedReaperTarget::TrackPeak(UnresolvedTrackPeakTarget {
                        track_descriptor: self.track_descriptor()?,
                    }),
                    TrackSendVolume => {
                        UnresolvedReaperTarget::TrackSendVolume(UnresolvedRouteVolumeTarget {
                            descriptor: self.track_route_descriptor()?,
                        })
                    }
                    TrackPan => UnresolvedReaperTarget::TrackPan(UnresolvedTrackPanTarget {
                        track_descriptor: self.track_descriptor()?,
                    }),
                    TrackWidth => UnresolvedReaperTarget::TrackWidth(UnresolvedTrackWidthTarget {
                        track_descriptor: self.track_descriptor()?,
                    }),
                    TrackArm => UnresolvedReaperTarget::TrackArm(UnresolvedTrackArmTarget {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity.get(),
                    }),
                    TrackSelection => {
                        UnresolvedReaperTarget::TrackSelection(UnresolvedTrackSelectionTarget {
                            track_descriptor: self.track_descriptor()?,
                            exclusivity: self.track_exclusivity.get(),
                            scroll_arrange_view: self.scroll_arrange_view.get(),
                            scroll_mixer: self.scroll_mixer.get(),
                        })
                    }
                    TrackMute => UnresolvedReaperTarget::TrackMute(UnresolvedTrackMuteTarget {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity.get(),
                    }),
                    TrackPhase => UnresolvedReaperTarget::TrackPhase(UnresolvedTrackPhaseTarget {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity.get(),
                        poll_for_feedback: self.poll_for_feedback.get(),
                    }),
                    TrackShow => UnresolvedReaperTarget::TrackShow(UnresolvedTrackShowTarget {
                        track_descriptor: self.track_descriptor()?,
                        exclusivity: self.track_exclusivity.get(),
                        area: match self.track_area.get() {
                            RealearnTrackArea::Tcp => TrackArea::Tcp,
                            RealearnTrackArea::Mcp => TrackArea::Mcp,
                        },
                        poll_for_feedback: self.poll_for_feedback.get(),
                    }),
                    TrackAutomationMode => UnresolvedReaperTarget::TrackAutomationMode(
                        UnresolvedTrackAutomationModeTarget {
                            track_descriptor: self.track_descriptor()?,
                            exclusivity: self.track_exclusivity.get(),
                            mode: self.automation_mode.get().to_reaper(),
                        },
                    ),
                    TrackSolo => UnresolvedReaperTarget::TrackSolo(UnresolvedTrackSoloTarget {
                        track_descriptor: self.track_descriptor()?,
                        behavior: self.solo_behavior.get(),
                        exclusivity: self.track_exclusivity.get(),
                    }),
                    TrackSendPan => {
                        UnresolvedReaperTarget::TrackSendPan(UnresolvedRoutePanTarget {
                            descriptor: self.track_route_descriptor()?,
                        })
                    }
                    TrackSendMute => {
                        UnresolvedReaperTarget::TrackSendMute(UnresolvedRouteMuteTarget {
                            descriptor: self.track_route_descriptor()?,
                            poll_for_feedback: self.poll_for_feedback.get(),
                        })
                    }
                    TrackSendPhase => {
                        UnresolvedReaperTarget::TrackRoutePhase(UnresolvedRoutePhaseTarget {
                            descriptor: self.track_route_descriptor()?,
                            poll_for_feedback: self.poll_for_feedback.get(),
                        })
                    }
                    TrackSendMono => {
                        UnresolvedReaperTarget::TrackRouteMono(UnresolvedRouteMonoTarget {
                            descriptor: self.track_route_descriptor()?,
                            poll_for_feedback: self.poll_for_feedback.get(),
                        })
                    }
                    TrackSendAutomationMode => UnresolvedReaperTarget::TrackRouteAutomationMode(
                        UnresolvedRouteAutomationModeTarget {
                            descriptor: self.track_route_descriptor()?,
                            mode: self.automation_mode.get().to_reaper(),
                            poll_for_feedback: self.poll_for_feedback.get(),
                        },
                    ),
                    Tempo => UnresolvedReaperTarget::Tempo(UnresolvedTempoTarget),
                    Playrate => UnresolvedReaperTarget::Playrate(UnresolvedPlayrateTarget),
                    AutomationModeOverride => UnresolvedReaperTarget::AutomationModeOverride(
                        UnresolvedAutomationModeOverrideTarget {
                            mode_override: match self.automation_mode_override_type.get() {
                                AutomationModeOverrideType::Bypass => {
                                    Some(GlobalAutomationModeOverride::Bypass)
                                }
                                AutomationModeOverrideType::Override => {
                                    Some(GlobalAutomationModeOverride::Mode(
                                        self.automation_mode.get().to_reaper(),
                                    ))
                                }
                                AutomationModeOverrideType::None => None,
                            },
                        },
                    ),
                    FxEnable => UnresolvedReaperTarget::FxEnable(UnresolvedFxEnableTarget {
                        fx_descriptor: self.fx_descriptor()?,
                    }),
                    FxOpen => UnresolvedReaperTarget::FxOpen(UnresolvedFxOpenTarget {
                        fx_descriptor: self.fx_descriptor()?,
                        display_type: self.fx_display_type.get(),
                    }),
                    FxPreset => UnresolvedReaperTarget::FxPreset(UnresolvedFxPresetTarget {
                        fx_descriptor: self.fx_descriptor()?,
                    }),
                    SelectedTrack => {
                        UnresolvedReaperTarget::SelectedTrack(UnresolvedSelectedTrackTarget {
                            scroll_arrange_view: self.scroll_arrange_view.get(),
                            scroll_mixer: self.scroll_mixer.get(),
                        })
                    }
                    FxNavigate => UnresolvedReaperTarget::FxNavigate(UnresolvedFxNavigateTarget {
                        track_descriptor: self.track_descriptor()?,
                        is_input_fx: self.fx_is_input_fx.get(),
                        display_type: self.fx_display_type.get(),
                    }),
                    AllTrackFxEnable => {
                        UnresolvedReaperTarget::AllTrackFxEnable(UnresolvedAllTrackFxEnableTarget {
                            track_descriptor: self.track_descriptor()?,
                            exclusivity: self.track_exclusivity.get(),
                            poll_for_feedback: self.poll_for_feedback.get(),
                        })
                    }
                    Transport => UnresolvedReaperTarget::Transport(UnresolvedTransportTarget {
                        action: self.transport_action.get(),
                    }),
                    LoadFxSnapshot => {
                        UnresolvedReaperTarget::LoadFxPreset(UnresolvedLoadFxSnapshotTarget {
                            fx_descriptor: self.fx_descriptor()?,
                            chunk: self
                                .fx_snapshot
                                .get_ref()
                                .as_ref()
                                .ok_or("FX chunk not set")?
                                .chunk
                                .clone(),
                        })
                    }
                    LastTouched => UnresolvedReaperTarget::LastTouched(UnresolvedLastTouchedTarget),
                    AutomationTouchState => UnresolvedReaperTarget::AutomationTouchState(
                        UnresolvedAutomationTouchStateTarget {
                            track_descriptor: self.track_descriptor()?,
                            parameter_type: self.touched_parameter_type.get(),
                            exclusivity: self.track_exclusivity.get(),
                        },
                    ),
                    GoToBookmark => {
                        UnresolvedReaperTarget::GoToBookmark(UnresolvedGoToBookmarkTarget {
                            bookmark_type: self.bookmark_type.get(),
                            bookmark_anchor_type: self.bookmark_anchor_type.get(),
                            bookmark_ref: self.bookmark_ref.get(),
                            set_time_selection: self.use_time_selection.get(),
                            set_loop_points: self.use_loop_points.get(),
                        })
                    }
                    Seek => UnresolvedReaperTarget::Seek(UnresolvedSeekTarget {
                        options: self.seek_options(),
                    }),
                    SendMidi => UnresolvedReaperTarget::SendMidi(UnresolvedMidiSendTarget {
                        pattern: self.raw_midi_pattern.get_ref().parse().unwrap_or_default(),
                        destination: self.send_midi_destination.get(),
                    }),
                    SendOsc => UnresolvedReaperTarget::SendOsc(UnresolvedOscSendTarget {
                        address_pattern: self.osc_address_pattern.get_ref().clone(),
                        arg_descriptor: self.osc_arg_descriptor(),
                        device_id: self.osc_dev_id.get(),
                    }),
                    ClipTransport => {
                        UnresolvedReaperTarget::ClipTransport(UnresolvedClipTransportTarget {
                            // TODO-medium Make it possible to pass direct HW output channel instead
                            track_descriptor: Some(self.track_descriptor()?),
                            slot_index: self.slot_index.get(),
                            action: self.transport_action.get(),
                            play_options: self.slot_play_options(),
                        })
                    }
                    ClipSeek => UnresolvedReaperTarget::ClipSeek(UnresolvedClipSeekTarget {
                        slot_index: self.slot_index.get(),
                        feedback_resolution: self.feedback_resolution.get(),
                    }),
                    ClipVolume => UnresolvedReaperTarget::ClipVolume(UnresolvedClipVolumeTarget {
                        slot_index: self.slot_index.get(),
                    }),
                    LoadMappingSnapshot => UnresolvedReaperTarget::LoadMappingSnapshot(
                        UnresolvedLoadMappingSnapshotTarget {
                            scope: TagScope {
                                tags: self.tags.get_ref().iter().cloned().collect(),
                            },
                            active_mappings_only: self.active_mappings_only.get(),
                        },
                    ),
                    EnableMappings => {
                        UnresolvedReaperTarget::EnableMappings(UnresolvedEnableMappingsTarget {
                            compartment,
                            scope: TagScope {
                                tags: self.tags.get_ref().iter().cloned().collect(),
                            },
                            exclusivity: self.exclusivity.get(),
                        })
                    }
                    EnableInstances => {
                        UnresolvedReaperTarget::EnableInstances(UnresolvedEnableInstancesTarget {
                            scope: TagScope {
                                tags: self.tags.get_ref().iter().cloned().collect(),
                            },
                            exclusivity: self.exclusivity.get(),
                        })
                    }
                    NavigateWithinGroup => UnresolvedReaperTarget::NavigateWithinGroup(
                        UnresolvedNavigateWithinGroupTarget {
                            compartment,
                            group_id: self.group_id.get(),
                            exclusivity: self.exclusivity.get().into(),
                        },
                    ),
                    AnyOn => UnresolvedReaperTarget::AnyOn(UnresolvedAnyOnTarget {
                        parameter: self.any_on_parameter.get(),
                    }),
                };
                Ok(UnresolvedCompoundMappingTarget::Reaper(target))
            }
            Virtual => {
                let virtual_target = VirtualTarget::new(self.create_control_element());
                Ok(UnresolvedCompoundMappingTarget::Virtual(virtual_target))
            }
        }
    }

    pub fn slot_play_options(&self) -> SlotPlayOptions {
        SlotPlayOptions {
            next_bar: self.next_bar.get(),
            buffered: self.buffered.get(),
        }
    }

    fn osc_arg_descriptor(&self) -> Option<OscArgDescriptor> {
        let arg_index = self.osc_arg_index.get()?;
        Some(OscArgDescriptor::new(
            arg_index,
            self.osc_arg_type_tag.get(),
            // Doesn't matter for sending
            false,
        ))
    }

    pub fn with_context<'a>(
        &'a self,
        context: ExtendedProcessorContext<'a>,
        compartment: MappingCompartment,
    ) -> TargetModelWithContext<'a> {
        TargetModelWithContext {
            target: self,
            context,
            compartment,
        }
    }

    pub fn supports_track(&self) -> bool {
        let target_type = self.r#type.get();
        if !target_type.supports_track() {
            return false;
        }
        self.supports_track_apart_from_type()
    }

    pub fn supports_track_must_be_selected(&self) -> bool {
        if !self.r#type.get().supports_track_must_be_selected() {
            return false;
        }
        self.supports_track_apart_from_type()
    }

    fn supports_track_apart_from_type(&self) -> bool {
        match self.r#type.get() {
            ReaperTargetType::ClipTransport => {
                use TransportAction::*;
                matches!(self.transport_action.get(), PlayStop | PlayPause)
            }
            ReaperTargetType::Action => self.with_track.get(),
            _ => true,
        }
    }

    pub fn supports_fx(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        self.r#type.get().supports_fx()
    }

    pub fn supports_route(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        self.r#type.get().supports_send()
    }

    pub fn supports_automation_mode(&self) -> bool {
        if !self.is_reaper() {
            return false;
        }
        use ReaperTargetType::*;
        match self.r#type.get() {
            TrackAutomationMode | TrackSendAutomationMode => true,
            AutomationModeOverride => {
                self.automation_mode_override_type.get() == AutomationModeOverrideType::Override
            }
            _ => false,
        }
    }

    pub fn create_control_element(&self) -> VirtualControlElement {
        self.control_element_type
            .get()
            .create_control_element(self.control_element_id.get())
    }

    fn is_reaper(&self) -> bool {
        self.category.get() == TargetCategory::Reaper
    }

    pub fn is_virtual(&self) -> bool {
        self.category.get() == TargetCategory::Virtual
    }

    fn command_id_label(&self) -> Cow<str> {
        match self.action.get_ref() {
            None => "-".into(),
            Some(action) => {
                if action.is_available() {
                    action.command_id().to_string().into()
                } else if let Some(command_name) = action.command_name() {
                    format!("<Not present> ({})", command_name.to_str()).into()
                } else {
                    "<Not present>".into()
                }
            }
        }
    }

    pub fn action(&self) -> Result<Action, &'static str> {
        let action = self.action.get_ref().as_ref().ok_or("action not set")?;
        if !action.is_available() {
            return Err("action not available");
        }
        Ok(action.clone())
    }

    pub fn action_name_label(&self) -> Cow<str> {
        match self.action().ok() {
            None => "-".into(),
            Some(a) => a.name().into_string().into(),
        }
    }
}

pub struct TargetModelFormatVeryShort<'a>(pub &'a TargetModel);

impl<'a> Display for TargetModelFormatVeryShort<'a> {
    /// Produces a short single-line name which is for example used to derive the automatic name.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.category.get() {
            TargetCategory::Reaper => {
                use ReaperTargetType::*;
                let tt = self.0.r#type.get();
                match tt {
                    ClipTransport | ClipSeek | ClipVolume => {
                        write!(
                            f,
                            "{}: Slot {}",
                            tt.short_name(),
                            self.0.slot_index.get() + 1
                        )
                    }
                    Action => match self.0.action().ok() {
                        None => write!(f, "Action {}", self.0.command_id_label()),
                        Some(a) => f.write_str(a.name().to_str()),
                    },
                    AutomationModeOverride => {
                        write!(f, "{}: ", tt.short_name())?;
                        use AutomationModeOverrideType::*;
                        let ovr_type = self.0.automation_mode_override_type.get();
                        match ovr_type {
                            None | Bypass => write!(f, "{}", ovr_type),
                            Override => write!(f, "{}", self.0.automation_mode.get()),
                        }
                    }
                    Transport => {
                        write!(f, "{}", self.0.transport_action.get())
                    }
                    AnyOn => {
                        write!(f, "{}", self.0.any_on_parameter.get())
                    }
                    GoToBookmark => {
                        let type_label = match self.0.bookmark_type.get() {
                            BookmarkType::Marker => "Marker",
                            BookmarkType::Region => "Region",
                        };
                        let bm_prefix = match self.0.bookmark_anchor_type.get() {
                            BookmarkAnchorType::Id => "",
                            BookmarkAnchorType::Index => "#",
                        };
                        write!(
                            f,
                            "Go to {} {}{}",
                            type_label,
                            bm_prefix,
                            self.0.bookmark_ref.get()
                        )
                    }
                    TrackAutomationMode => {
                        write!(f, "{}: {}", tt.short_name(), self.0.automation_mode.get())
                    }
                    AutomationTouchState => write!(
                        f,
                        "{}: {}",
                        tt.short_name(),
                        self.0.touched_parameter_type.get()
                    ),
                    _ => f.write_str(tt.short_name()),
                }
            }
            TargetCategory::Virtual => match self.0.control_element_id.get() {
                VirtualControlElementId::Indexed(i) => {
                    write!(f, "{} {}", self.0.control_element_type.get(), i + 1)
                }
                VirtualControlElementId::Named(n) => {
                    write!(f, "{} ({})", n, self.0.control_element_type.get())
                }
            },
        }
    }
}

pub struct TargetModelFormatMultiLine<'a> {
    target: &'a TargetModel,
    context: ExtendedProcessorContext<'a>,
    compartment: MappingCompartment,
}

impl<'a> TargetModelFormatMultiLine<'a> {
    pub fn new(
        target: &'a TargetModel,
        context: ExtendedProcessorContext<'a>,
        compartment: MappingCompartment,
    ) -> Self {
        TargetModelFormatMultiLine {
            target,
            context,
            compartment,
        }
    }

    fn track_label(&self) -> String {
        let virtual_track = self.target.virtual_track();
        let virtual_track = match virtual_track.as_ref() {
            None => return TARGET_UNDEFINED_LABEL.into(),
            Some(t) => t,
        };
        use VirtualTrack::*;
        match virtual_track {
            ById(_) | ByIdOrName(_, _) => {
                if let Ok(t) = self.target_with_context().first_effective_track() {
                    get_track_label(&t)
                } else {
                    get_non_present_virtual_track_label(virtual_track)
                }
            }
            _ => virtual_track.to_string(),
        }
    }

    fn route_label(&self) -> Cow<str> {
        let virtual_route = self.target.virtual_track_route().ok();
        let virtual_route = match virtual_route.as_ref() {
            None => return TARGET_UNDEFINED_LABEL.into(),
            Some(r) => r,
        };
        use TrackRouteSelector::*;
        match &virtual_route.selector {
            ById(_) => {
                if let Ok(r) = self.resolve_track_route() {
                    get_route_label(&r).into()
                } else {
                    get_non_present_virtual_route_label(virtual_route).into()
                }
            }
            _ => virtual_route.to_string().into(),
        }
    }

    fn fx_label(&self) -> Cow<str> {
        let virtual_fx = self.target.virtual_fx();
        let virtual_fx = match virtual_fx.as_ref() {
            None => return TARGET_UNDEFINED_LABEL.into(),
            Some(f) => f,
        };
        match virtual_fx {
            VirtualFx::ChainFx { chain_fx, .. } => {
                use VirtualChainFx::*;
                match chain_fx {
                    ById(_, _) | ByIdOrIndex(_, _) => get_optional_fx_label(
                        chain_fx,
                        self.target_with_context().first_fx().ok().as_ref(),
                    )
                    .into(),
                    _ => virtual_fx.to_string().into(),
                }
            }
            _ => virtual_fx.to_string().into(),
        }
    }

    fn fx_param_label(&self) -> Cow<str> {
        let virtual_param = self.target.virtual_fx_parameter();
        let virtual_param = match virtual_param.as_ref() {
            None => return TARGET_UNDEFINED_LABEL.into(),
            Some(p) => p,
        };
        use VirtualFxParameter::*;
        match virtual_param {
            ById(_) => {
                if let Ok(p) = self.resolve_fx_param() {
                    get_fx_param_label(Some(&p), p.index())
                } else {
                    format!("<Not present> ({})", virtual_param).into()
                }
            }
            _ => virtual_param.to_string().into(),
        }
    }

    fn bookmark_label(&self) -> String {
        // TODO-medium We should do this similar to the other target objects and introduce a
        //  virtual struct.
        let bookmark_type = self.target.bookmark_type.get();
        {
            let anchor_type = self.target.bookmark_anchor_type.get();
            let bookmark_ref = self.target.bookmark_ref.get();
            let res = find_bookmark(
                self.context.context().project_or_current_project(),
                bookmark_type,
                anchor_type,
                bookmark_ref,
            );
            if let Ok(res) = res {
                get_bookmark_label(
                    res.index_within_type,
                    res.basic_info.id,
                    &res.bookmark.name(),
                )
            } else {
                get_non_present_bookmark_label(anchor_type, bookmark_ref)
            }
        }
    }

    // Returns an error if that send (or track) doesn't exist.
    pub fn resolve_track_route(&self) -> Result<TrackRoute, &'static str> {
        get_track_route(
            self.context,
            &self.target.track_route_descriptor()?,
            self.compartment,
        )
    }

    // Returns an error if that param (or FX) doesn't exist.
    fn resolve_fx_param(&self) -> Result<FxParameter, &'static str> {
        get_fx_param(
            self.context,
            &self.target.fx_parameter_descriptor()?,
            self.compartment,
        )
    }

    fn target_with_context(&self) -> TargetModelWithContext<'a> {
        self.target.with_context(self.context, self.compartment)
    }
}

impl<'a> Display for TargetModelFormatMultiLine<'a> {
    /// Produces a multi-line description of the target.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use TargetCategory::*;
        match self.target.category.get() {
            Reaper => {
                use ReaperTargetType::*;
                let tt = self.target.r#type.get();
                match tt {
                    ClipTransport | ClipSeek | ClipVolume => {
                        write!(f, "{}", tt)
                    }
                    Action => write!(
                        f,
                        "{}\n{}\n{}",
                        tt,
                        self.target.command_id_label(),
                        self.target.action_name_label()
                    ),
                    FxParameter => write!(
                        f,
                        "{}\nTrack {}\nFX {}\nParam {}",
                        tt,
                        self.track_label(),
                        self.fx_label(),
                        self.fx_param_label()
                    ),
                    TrackTool | TrackVolume | TrackPeak | TrackPan | TrackWidth | TrackArm
                    | TrackSelection | TrackMute | TrackPhase | TrackSolo | TrackShow
                    | FxNavigate | AllTrackFxEnable => {
                        write!(f, "{}\nTrack {}", tt, self.track_label())
                    }
                    TrackAutomationMode => {
                        write!(
                            f,
                            "{}\nTrack {}\n{}",
                            tt,
                            self.track_label(),
                            self.target.automation_mode.get()
                        )
                    }
                    TrackSendVolume
                    | TrackSendPan
                    | TrackSendMute
                    | TrackSendPhase
                    | TrackSendMono
                    | TrackSendAutomationMode => write!(
                        f,
                        "{}\nTrack {}\n{} {}",
                        tt,
                        self.track_label(),
                        self.target.route_type.get(),
                        self.route_label()
                    ),
                    FxOpen | FxEnable | FxPreset => write!(
                        f,
                        "{}\nTrack {}\nFX {}",
                        tt,
                        self.track_label(),
                        self.fx_label(),
                    ),
                    Transport => write!(f, "{}\n{}", tt, self.target.transport_action.get()),
                    AnyOn => write!(f, "{}\n{}", tt, self.target.any_on_parameter.get()),
                    AutomationModeOverride => {
                        write!(
                            f,
                            "{}\n{}",
                            tt,
                            self.target.automation_mode_override_type.get()
                        )
                    }
                    LoadFxSnapshot => write!(
                        f,
                        "{}\n{}",
                        tt,
                        self.target
                            .fx_snapshot
                            .get_ref()
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| "-".to_owned())
                    ),
                    AutomationTouchState => write!(
                        f,
                        "{}\nTrack {}\n{}",
                        tt,
                        self.track_label(),
                        self.target.touched_parameter_type.get()
                    ),
                    GoToBookmark => {
                        write!(f, "{}\n{}", tt, self.bookmark_label())
                    }
                    _ => write!(f, "{}", tt),
                }
            }
            Virtual => write!(f, "Virtual\n{}", self.target.create_control_element()),
        }
    }
}

pub fn get_fx_param_label(fx_param: Option<&FxParameter>, index: u32) -> Cow<'static, str> {
    let position = index + 1;
    match fx_param {
        None => format!("{}. <Not present>", position).into(),
        Some(p) => {
            let name = p.name().into_inner();
            // Parameter names are not reliably UTF-8-encoded (e.g. "JS: Stereo Width")
            let name = name.to_string_lossy();
            if name.is_empty() {
                position.to_string().into()
            } else {
                format!("{}. {}", position, name).into()
            }
        }
    }
}

pub fn get_route_label(route: &TrackRoute) -> String {
    format!("{}. {}", route.index() + 1, route.name().to_str())
}

pub fn get_optional_fx_label(virtual_chain_fx: &VirtualChainFx, fx: Option<&Fx>) -> String {
    match virtual_chain_fx {
        VirtualChainFx::Dynamic(_) => virtual_chain_fx.to_string(),
        _ => match fx {
            None => format!("<Not present> ({})", virtual_chain_fx),
            Some(fx) => get_fx_label(fx.index(), fx),
        },
    }
}

pub fn get_fx_label(index: u32, fx: &Fx) -> String {
    format!(
        "{}. {}",
        index + 1,
        // When closing project, this is sometimes not available anymore although the FX is still
        // picked up when querying the list of FXs! Prevent a panic.
        if fx.is_available() {
            fx.name().into_string()
        } else {
            "".to_owned()
        }
    )
}

pub struct TargetModelWithContext<'a> {
    target: &'a TargetModel,
    context: ExtendedProcessorContext<'a>,
    compartment: MappingCompartment,
}

impl<'a> TargetModelWithContext<'a> {
    /// Creates a target based on this model's properties and the current REAPER state.
    ///
    /// This returns a target regardless of the activation conditions of the target. Example:
    /// If `enable_only_if_track_selected` is `true` and the track is _not_ selected when calling
    /// this function, the target will still be created!
    ///
    /// # Errors
    ///
    /// Returns an error if not enough information is provided by the model or if something (e.g.
    /// track/FX/parameter) is not available.
    pub fn resolve(&self) -> Result<Vec<CompoundMappingTarget>, &'static str> {
        let unresolved = self.target.create_target(self.compartment)?;
        unresolved.resolve(self.context, self.compartment)
    }

    pub fn resolve_first(&self) -> Result<CompoundMappingTarget, &'static str> {
        let targets = self.resolve()?;
        targets.into_iter().next().ok_or("resolved to empty list")
    }

    pub fn is_known_to_be_roundable(&self) -> bool {
        // TODO-low use cached
        self.resolve_first()
            .map(|t| {
                matches!(
                    t.control_type(self.context.control_context()),
                    ControlType::AbsoluteContinuousRoundable { .. }
                )
            })
            .unwrap_or(false)
    }
    // Returns an error if the FX doesn't exist.
    pub fn first_fx(&self) -> Result<Fx, &'static str> {
        get_fxs(
            self.context,
            &self.target.fx_descriptor()?,
            self.compartment,
        )?
        .into_iter()
        .next()
        .ok_or("resolves to empty FX list")
    }

    pub fn project(&self) -> Project {
        self.context.context().project_or_current_project()
    }

    pub fn first_effective_track(&self) -> Result<Track, &'static str> {
        self.target
            .virtual_track()
            .ok_or("virtual track not complete")?
            .resolve(self.context, self.compartment)
            .map_err(|_| "particular track couldn't be resolved")?
            .into_iter()
            .next()
            .ok_or("resolved to empty track list")
    }
}

pub fn get_bookmark_label(index_within_type: u32, id: BookmarkId, name: &str) -> String {
    format!("{}. {} (ID {})", index_within_type + 1, name, id)
}

pub fn get_non_present_bookmark_label(
    anchor_type: BookmarkAnchorType,
    bookmark_ref: u32,
) -> String {
    match anchor_type {
        BookmarkAnchorType::Id => format!("<Not present> (ID {})", bookmark_ref),
        BookmarkAnchorType::Index => format!("{}. <Not present>", bookmark_ref),
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TargetCategory {
    #[serde(rename = "reaper")]
    #[display(fmt = "Real")]
    Reaper,
    #[serde(rename = "virtual")]
    #[display(fmt = "Virtual")]
    Virtual,
}

impl TargetCategory {
    pub fn default_for(compartment: MappingCompartment) -> Self {
        use TargetCategory::*;
        match compartment {
            MappingCompartment::ControllerMappings => Virtual,
            MappingCompartment::MainMappings => Reaper,
        }
    }

    pub fn is_allowed_in(self, compartment: MappingCompartment) -> bool {
        use TargetCategory::*;
        match compartment {
            MappingCompartment::ControllerMappings => true,
            MappingCompartment::MainMappings => match self {
                Reaper => true,
                Virtual => false,
            },
        }
    }
}

impl Default for TargetCategory {
    fn default() -> Self {
        TargetCategory::Reaper
    }
}

fn virtualize_track(
    track: &Track,
    context: &ProcessorContext,
    special_monitoring_fx_handling: bool,
) -> VirtualTrack {
    let own_track = context
        .track()
        .cloned()
        .unwrap_or_else(|| context.project_or_current_project().master_track());
    if own_track == *track {
        VirtualTrack::This
    } else if track.is_master_track() {
        VirtualTrack::Master
    } else if special_monitoring_fx_handling && context.is_on_monitoring_fx_chain() {
        // Doesn't make sense to refer to tracks via ID if we are on monitoring FX chain.
        VirtualTrack::ByIndex(track.index().expect("impossible"))
    } else {
        VirtualTrack::ById(*track.guid())
    }
}

fn virtualize_fx(
    fx: &Fx,
    context: &ProcessorContext,
    special_monitoring_fx_handling: bool,
) -> VirtualFx {
    if context.containing_fx() == fx {
        VirtualFx::This
    } else {
        VirtualFx::ChainFx {
            is_input_fx: fx.is_input_fx(),
            chain_fx: if special_monitoring_fx_handling && context.is_on_monitoring_fx_chain() {
                // Doesn't make sense to refer to FX via UUID if we are on monitoring FX chain.
                VirtualChainFx::ByIndex(fx.index())
            } else if let Some(guid) = fx.guid() {
                VirtualChainFx::ById(guid, Some(fx.index()))
            } else {
                // This can happen if the incoming FX was created in an index-based way.
                // TODO-medium We really should use separate types in reaper-high!
                let guid = fx.chain().fx_by_index(fx.index()).and_then(|f| f.guid());
                if let Some(guid) = guid {
                    VirtualChainFx::ById(guid, Some(fx.index()))
                } else {
                    VirtualChainFx::ByIdOrIndex(None, fx.index())
                }
            },
        }
    }
}

fn virtualize_route(
    route: &TrackRoute,
    context: &ProcessorContext,
    special_monitoring_fx_handling: bool,
) -> VirtualTrackRoute {
    let partner = route.partner();
    VirtualTrackRoute {
        r#type: match route.direction() {
            TrackSendDirection::Receive => TrackRouteType::Receive,
            TrackSendDirection::Send => {
                if matches!(partner, Some(TrackRoutePartner::HardwareOutput(_))) {
                    TrackRouteType::HardwareOutput
                } else {
                    TrackRouteType::Send
                }
            }
        },
        selector: if special_monitoring_fx_handling && context.is_on_monitoring_fx_chain() {
            // Doesn't make sense to refer to route via related-track UUID if we are on monitoring
            // FX chain.
            TrackRouteSelector::ByIndex(route.index())
        } else {
            match partner {
                None | Some(TrackRoutePartner::HardwareOutput(_)) => {
                    TrackRouteSelector::ByIndex(route.index())
                }
                Some(TrackRoutePartner::Track(t)) => TrackRouteSelector::ById(*t.guid()),
            }
        },
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, IntoEnumIterator, TryFromPrimitive, IntoPrimitive, Display,
)]
#[repr(usize)]
pub enum VirtualTrackType {
    #[display(fmt = "<This>")]
    This,
    #[display(fmt = "<Selected>")]
    Selected,
    #[display(fmt = "<All selected>")]
    AllSelected,
    #[display(fmt = "<Dynamic>")]
    Dynamic,
    #[display(fmt = "<Master>")]
    Master,
    #[display(fmt = "By ID")]
    ById,
    #[display(fmt = "By name")]
    ByName,
    #[display(fmt = "All by name")]
    AllByName,
    #[display(fmt = "By position")]
    ByIndex,
    #[display(fmt = "By ID or name")]
    ByIdOrName,
}

impl Default for VirtualTrackType {
    fn default() -> Self {
        Self::This
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum BookmarkAnchorType {
    #[display(fmt = "By ID")]
    Id,
    #[display(fmt = "By position")]
    Index,
}

impl Default for BookmarkAnchorType {
    fn default() -> Self {
        Self::Id
    }
}

impl VirtualTrackType {
    pub fn from_virtual_track(virtual_track: &VirtualTrack) -> Self {
        use VirtualTrack::*;
        match virtual_track {
            This => Self::This,
            Selected { allow_multiple } => {
                if *allow_multiple {
                    Self::AllSelected
                } else {
                    Self::Selected
                }
            }
            Dynamic(_) => Self::Dynamic,
            Master => Self::Master,
            ByIdOrName(_, _) => Self::ByIdOrName,
            ById(_) => Self::ById,
            ByName { allow_multiple, .. } => {
                if *allow_multiple {
                    Self::AllByName
                } else {
                    Self::ByName
                }
            }
            ByIndex(_) => Self::ByIndex,
        }
    }

    pub fn refers_to_project(&self) -> bool {
        use VirtualTrackType::*;
        matches!(self, ByIdOrName | ById)
    }

    pub fn is_sticky(&self) -> bool {
        use VirtualTrackType::*;
        matches!(self, ByIdOrName | ById | This | Master)
    }

    pub fn track_selected_condition_makes_sense(&self) -> bool {
        use VirtualTrackType::*;
        !matches!(self, Selected | AllSelected)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum VirtualFxType {
    #[display(fmt = "<This>")]
    #[serde(rename = "this")]
    This,
    #[display(fmt = "<Focused>")]
    #[serde(rename = "focused")]
    Focused,
    #[display(fmt = "<Dynamic>")]
    #[serde(rename = "dynamic")]
    Dynamic,
    #[display(fmt = "By ID")]
    #[serde(rename = "id")]
    ById,
    #[display(fmt = "By name")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "All by name")]
    AllByName,
    #[display(fmt = "By position")]
    #[serde(rename = "index")]
    ByIndex,
    #[display(fmt = "By ID or pos")]
    #[serde(rename = "id-or-index")]
    ByIdOrIndex,
}

impl Default for VirtualFxType {
    fn default() -> Self {
        Self::ById
    }
}

impl VirtualFxType {
    pub fn from_virtual_fx(virtual_fx: &VirtualFx) -> Self {
        use VirtualFx::*;
        match virtual_fx {
            This => VirtualFxType::This,
            Focused => VirtualFxType::Focused,
            ChainFx { chain_fx, .. } => {
                use VirtualChainFx::*;
                match chain_fx {
                    Dynamic(_) => Self::Dynamic,
                    ById(_, _) => Self::ById,
                    ByName { allow_multiple, .. } => {
                        if *allow_multiple {
                            Self::AllByName
                        } else {
                            Self::ByName
                        }
                    }
                    ByIndex(_) => Self::ByIndex,
                    ByIdOrIndex(_, _) => Self::ByIdOrIndex,
                }
            }
        }
    }

    pub fn refers_to_project(&self) -> bool {
        use VirtualFxType::*;
        matches!(self, ById | ByIdOrIndex)
    }

    pub fn is_sticky(&self) -> bool {
        use VirtualFxType::*;
        matches!(self, ById | ByIdOrIndex | This)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum VirtualFxParameterType {
    #[display(fmt = "<Dynamic>")]
    #[serde(rename = "dynamic")]
    Dynamic,
    #[display(fmt = "By name")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "By ID")]
    #[serde(rename = "index")]
    ById,
    #[display(fmt = "By position")]
    #[serde(rename = "index-manual")]
    ByIndex,
}

impl Default for VirtualFxParameterType {
    fn default() -> Self {
        Self::ById
    }
}

impl VirtualFxParameterType {
    pub fn from_virtual_fx_parameter(param: &VirtualFxParameter) -> Self {
        use VirtualFxParameter::*;
        match param {
            Dynamic(_) => Self::Dynamic,
            ByName(_) => Self::ByName,
            ByIndex(_) => Self::ByIndex,
            ById(_) => Self::ById,
        }
    }

    pub fn is_sticky(&self) -> bool {
        use VirtualFxParameterType::*;
        matches!(self, ById)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
    Serialize,
    Deserialize,
)]
#[repr(usize)]
pub enum TrackRouteSelectorType {
    #[display(fmt = "<Dynamic>")]
    #[serde(rename = "dynamic")]
    Dynamic,
    #[display(fmt = "By ID")]
    #[serde(rename = "id")]
    ById,
    #[display(fmt = "By name")]
    #[serde(rename = "name")]
    ByName,
    #[display(fmt = "By position")]
    #[serde(rename = "index")]
    ByIndex,
}

impl Default for TrackRouteSelectorType {
    fn default() -> Self {
        Self::ByIndex
    }
}

impl TrackRouteSelectorType {
    pub fn from_route_selector(selector: &TrackRouteSelector) -> Self {
        use TrackRouteSelector::*;
        match selector {
            Dynamic(_) => Self::Dynamic,
            ById(_) => Self::ById,
            ByName(_) => Self::ByName,
            ByIndex(_) => Self::ByIndex,
        }
    }

    pub fn refers_to_project(&self) -> bool {
        use TrackRouteSelectorType::*;
        matches!(self, ById)
    }

    pub fn is_sticky(&self) -> bool {
        use TrackRouteSelectorType::*;
        matches!(self, ById)
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxSnapshot {
    #[serde(default, skip_serializing_if = "is_default")]
    pub fx_type: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub fx_name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub preset_name: Option<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub chunk: Rc<String>,
}

impl Clone for FxSnapshot {
    fn clone(&self) -> Self {
        Self {
            fx_type: self.fx_type.clone(),
            fx_name: self.fx_name.clone(),
            preset_name: self.preset_name.clone(),
            // We want a totally detached duplicate.
            chunk: Rc::new((*self.chunk).clone()),
        }
    }
}

impl Display for FxSnapshot {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let fmt_size = bytesize::ByteSize(self.chunk.len() as _);
        write!(
            f,
            "{} | {} | {}",
            self.preset_name.as_deref().unwrap_or("-"),
            fmt_size,
            self.fx_name,
        )
    }
}

#[derive(Default)]
pub struct TrackPropValues {
    pub r#type: VirtualTrackType,
    pub id: Option<Guid>,
    pub name: String,
    pub expression: String,
    pub index: u32,
}

impl TrackPropValues {
    pub fn from_virtual_track(track: VirtualTrack) -> Self {
        Self {
            r#type: VirtualTrackType::from_virtual_track(&track),
            id: track.id(),
            name: track.name().unwrap_or_default(),
            index: track.index().unwrap_or_default(),
            expression: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct TrackRoutePropValues {
    pub selector_type: TrackRouteSelectorType,
    pub r#type: TrackRouteType,
    pub id: Option<Guid>,
    pub name: String,
    pub expression: String,
    pub index: u32,
}

impl TrackRoutePropValues {
    pub fn from_virtual_route(route: VirtualTrackRoute) -> Self {
        Self {
            selector_type: TrackRouteSelectorType::from_route_selector(&route.selector),
            r#type: route.r#type,
            id: route.id(),
            name: route.name().unwrap_or_default(),
            index: route.index().unwrap_or_default(),
            expression: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct FxPropValues {
    pub r#type: VirtualFxType,
    pub is_input_fx: bool,
    pub id: Option<Guid>,
    pub name: String,
    pub expression: String,
    pub index: u32,
}

impl FxPropValues {
    pub fn from_virtual_fx(fx: VirtualFx) -> Self {
        Self {
            r#type: VirtualFxType::from_virtual_fx(&fx),
            is_input_fx: fx.is_input_fx(),
            id: fx.id(),
            name: fx.name().unwrap_or_default(),
            index: fx.index().unwrap_or_default(),
            expression: Default::default(),
        }
    }
}

#[derive(Default)]
pub struct FxParameterPropValues {
    pub r#type: VirtualFxParameterType,
    pub name: String,
    pub expression: String,
    pub index: u32,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum RealearnTrackArea {
    #[serde(rename = "tcp")]
    #[display(fmt = "Track control panel")]
    Tcp,
    #[serde(rename = "mcp")]
    #[display(fmt = "Mixer control panel")]
    Mcp,
}

impl Default for RealearnTrackArea {
    fn default() -> Self {
        Self::Tcp
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum RealearnAutomationMode {
    #[display(fmt = "Trim/Read")]
    TrimRead = 0,
    #[display(fmt = "Read")]
    Read = 1,
    #[display(fmt = "Touch")]
    Touch = 2,
    #[display(fmt = "Write")]
    Write = 3,
    #[display(fmt = "Latch")]
    Latch = 4,
    #[display(fmt = "Latch Preview")]
    LatchPreview = 5,
}

impl Default for RealearnAutomationMode {
    fn default() -> Self {
        Self::TrimRead
    }
}

impl RealearnAutomationMode {
    fn to_reaper(self) -> AutomationMode {
        use RealearnAutomationMode::*;
        match self {
            TrimRead => AutomationMode::TrimRead,
            Read => AutomationMode::Read,
            Touch => AutomationMode::Touch,
            Write => AutomationMode::Write,
            Latch => AutomationMode::Latch,
            LatchPreview => AutomationMode::LatchPreview,
        }
    }

    fn from_reaper(value: AutomationMode) -> Self {
        use AutomationMode::*;
        match value {
            TrimRead => Self::TrimRead,
            Read => Self::Read,
            Touch => Self::Touch,
            Write => Self::Write,
            Latch => Self::Latch,
            LatchPreview => Self::LatchPreview,
            Unknown(_) => Self::TrimRead,
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    Serialize,
    Deserialize,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum AutomationModeOverrideType {
    #[serde(rename = "none")]
    #[display(fmt = "None")]
    None,
    #[serde(rename = "bypass")]
    #[display(fmt = "Bypass all envelopes")]
    Bypass,
    #[serde(rename = "override")]
    #[display(fmt = "Override")]
    Override,
}

impl Default for AutomationModeOverrideType {
    fn default() -> Self {
        Self::Bypass
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    IntoEnumIterator,
    Serialize,
    Deserialize,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TargetUnit {
    #[serde(rename = "native")]
    Native,
    #[serde(rename = "percent")]
    Percent,
}

impl Default for TargetUnit {
    fn default() -> Self {
        Self::Native
    }
}

#[derive(Debug)]
pub enum ConcreteTrackInstruction<'a> {
    /// If the context is not available, other track properties won't get set.
    This(Option<&'a ProcessorContext>),
    /// If the context is not available, other track properties won't get set.
    ById {
        id: Option<Guid>,
        context: Option<&'a ProcessorContext>,
    },
    ByIdWithTrack(Track),
}

impl<'a> ConcreteTrackInstruction<'a> {
    pub fn resolve(self) -> ResolvedConcreteTrackInstruction<'a> {
        use ConcreteTrackInstruction::*;
        ResolvedConcreteTrackInstruction {
            track: match &self {
                This(context) => context.and_then(|c| c.track().cloned()),
                ById {
                    id: Some(id),
                    context: Some(c),
                } => {
                    let t = c.project_or_current_project().track_by_guid(id);
                    if t.is_available() {
                        Some(t)
                    } else {
                        None
                    }
                }
                ByIdWithTrack(t) => Some(t.clone()),
                _ => None,
            },
            instruction: self,
        }
    }
}

pub struct ResolvedConcreteTrackInstruction<'a> {
    instruction: ConcreteTrackInstruction<'a>,
    track: Option<Track>,
}

impl<'a> ResolvedConcreteTrackInstruction<'a> {
    pub fn virtual_track_type(&self) -> VirtualTrackType {
        use ConcreteTrackInstruction::*;
        match &self.instruction {
            This(_) => VirtualTrackType::This,
            ById { .. } | ByIdWithTrack(_) => VirtualTrackType::ById,
        }
    }

    pub fn id(&self) -> Option<Guid> {
        use ConcreteTrackInstruction::*;
        match &self.instruction {
            ById { id, .. } => *id,
            _ => Some(*self.track.as_ref()?.guid()),
        }
    }

    pub fn name(&self) -> Option<String> {
        Some(self.track.as_ref()?.name()?.into_string())
    }

    pub fn index(&self) -> Option<u32> {
        self.track.as_ref()?.index()
    }
}

#[derive(Debug)]
pub enum ConcreteFxInstruction<'a> {
    /// If the context is not available, other FX properties won't get set.
    This(Option<&'a ProcessorContext>),
    /// If the context is not available, other FX properties won't get set.
    ById {
        is_input_fx: Option<bool>,
        id: Option<Guid>,
        track: Option<Track>,
    },
    ByIdWithFx(Fx),
}

impl<'a> ConcreteFxInstruction<'a> {
    pub fn resolve(self) -> ResolvedConcreteFxInstruction<'a> {
        use ConcreteFxInstruction::*;
        ResolvedConcreteFxInstruction {
            fx: match &self {
                This(context) => context.map(|c| c.containing_fx().clone()),
                ById {
                    is_input_fx: Some(is_input_fx),
                    id: Some(id),
                    track: Some(t),
                } => {
                    let chain = if *is_input_fx {
                        t.input_fx_chain()
                    } else {
                        t.normal_fx_chain()
                    };
                    let fx = chain.fx_by_guid(id);
                    if fx.is_available() {
                        Some(fx)
                    } else {
                        None
                    }
                }
                ByIdWithFx(fx) => Some(fx.clone()),
                _ => None,
            },
            instruction: self,
        }
    }
}

pub struct ResolvedConcreteFxInstruction<'a> {
    instruction: ConcreteFxInstruction<'a>,
    fx: Option<Fx>,
}

impl<'a> ResolvedConcreteFxInstruction<'a> {
    pub fn virtual_fx_type(&self) -> VirtualFxType {
        use ConcreteFxInstruction::*;
        match self.instruction {
            This(_) => VirtualFxType::This,
            ById { .. } | ByIdWithFx(_) => VirtualFxType::ById,
        }
    }

    pub fn is_input_fx(&self) -> Option<bool> {
        use ConcreteFxInstruction::*;
        match &self.instruction {
            ById { is_input_fx, .. } => *is_input_fx,
            _ => Some(self.fx.as_ref()?.is_input_fx()),
        }
    }

    pub fn id(&self) -> Option<Guid> {
        use ConcreteFxInstruction::*;
        match &self.instruction {
            ById { id, .. } => *id,
            _ => self.fx.as_ref()?.guid(),
        }
    }

    pub fn name(&self) -> Option<String> {
        Some(self.fx.as_ref()?.name().into_string())
    }

    pub fn index(&self) -> Option<u32> {
        Some(self.fx.as_ref()?.index())
    }
}

const TARGET_UNDEFINED_LABEL: &str = "<Undefined>";

fn get_track_label(track: &Track) -> String {
    match track.location() {
        TrackLocation::MasterTrack => "<Master track>".into(),
        TrackLocation::NormalTrack(i) => {
            let position = i + 1;
            let name = track.name().expect("non-master track must have name");
            let name = name.to_str();
            if name.is_empty() {
                position.to_string()
            } else {
                format!("{}. {}", position, name)
            }
        }
    }
}
