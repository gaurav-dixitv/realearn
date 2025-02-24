use crate::base::default_util::is_default;
use derive_more::Display;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, NumericValue, PropValue, Target, UnitValue,
};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{
    Action, AvailablePanValue, BookmarkType, ChangeEvent, Fx, FxChain, Pan, PlayRate, Project,
    Reaper, Tempo, Track, TrackRoute, Width,
};
use reaper_medium::{
    AutomationMode, Bpm, GlobalAutomationModeOverride, NormalizedPlayRate, PlaybackSpeedFactor,
    PositionInSeconds, ReaperPanValue, ReaperWidthValue,
};
use rxrust::prelude::*;

use crate::domain::{
    AnyOnTarget, CompoundChangeEvent, EnableInstancesTarget, EnableMappingsTarget,
    HitInstructionReturnValue, LoadMappingSnapshotTarget, NavigateWithinGroupTarget,
    RealearnTarget, ReaperTargetType, RouteAutomationModeTarget, RouteMonoTarget, RoutePhaseTarget,
    TrackPhaseTarget, TrackToolTarget,
};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::base::Global;
use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    handle_exclusivity, ActionTarget, AllTrackFxEnableTarget, AutomationModeOverrideTarget,
    AutomationTouchStateTarget, ClipPlayState, ClipSeekTarget, ClipTransportTarget,
    ClipVolumeTarget, ControlContext, FxEnableTarget, FxNavigateTarget, FxOpenTarget,
    FxParameterTarget, FxPresetTarget, GoToBookmarkTarget, HierarchyEntry, HierarchyEntryProvider,
    LoadFxSnapshotTarget, MappingControlContext, MidiSendTarget, OscSendTarget, PlayrateTarget,
    RouteMuteTarget, RoutePanTarget, RouteVolumeTarget, SeekTarget, SelectedTrackTarget,
    TempoTarget, TrackArmTarget, TrackAutomationModeTarget, TrackMuteTarget, TrackPanTarget,
    TrackPeakTarget, TrackSelectionTarget, TrackShowTarget, TrackSoloTarget, TrackVolumeTarget,
    TrackWidthTarget, TransportTarget,
};
use enum_dispatch::enum_dispatch;
use std::convert::TryInto;
use std::rc::Rc;

/// This target character is just used for GUI and auto-correct settings! It doesn't have influence
/// on control/feedback.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum TargetCharacter {
    /// "Fire-only", so like switch but whenever it only makes sense to send "on", not "off".
    ///
    /// Rendered as one button.
    Trigger,
    /// When there are just two states: "on" and "off".
    ///
    /// Rendered as two buttons.
    Switch,
    /// Whenever there's a certain, discrete number of target values (steps).
    Discrete,
    /// Whenever the step size between two target values can get arbitrarily small.
    Continuous,
    /// When the target is a virtual control element that allows for more than 2 states.
    VirtualMulti,
    /// When the target is a virtual control element that allows for a maximum of 2 states.
    VirtualButton,
}

impl TargetCharacter {
    pub fn is_button_like(self) -> bool {
        use TargetCharacter::*;
        matches!(self, Trigger | Switch | VirtualButton)
    }
}

/// This is a ReaLearn target.
///
/// Unlike TargetModel, the real target has everything resolved already (e.g. track and FX) and
/// is immutable.
// TODO-medium Rename to RealTarget
#[enum_dispatch]
#[derive(Clone, Debug, PartialEq)]
pub enum ReaperTarget {
    Action(ActionTarget),
    FxParameter(FxParameterTarget),
    TrackVolume(TrackVolumeTarget),
    TrackTool(TrackToolTarget),
    TrackPeak(TrackPeakTarget),
    TrackRouteVolume(RouteVolumeTarget),
    TrackPan(TrackPanTarget),
    TrackWidth(TrackWidthTarget),
    TrackArm(TrackArmTarget),
    TrackSelection(TrackSelectionTarget),
    TrackMute(TrackMuteTarget),
    TrackPhase(TrackPhaseTarget),
    TrackShow(TrackShowTarget),
    TrackSolo(TrackSoloTarget),
    TrackAutomationMode(TrackAutomationModeTarget),
    TrackRoutePan(RoutePanTarget),
    TrackRouteMute(RouteMuteTarget),
    TrackRoutePhase(RoutePhaseTarget),
    TrackRouteMono(RouteMonoTarget),
    TrackRouteAutomationMode(RouteAutomationModeTarget),
    Tempo(TempoTarget),
    Playrate(PlayrateTarget),
    AutomationModeOverride(AutomationModeOverrideTarget),
    FxEnable(FxEnableTarget),
    FxOpen(FxOpenTarget),
    FxPreset(FxPresetTarget),
    SelectedTrack(SelectedTrackTarget),
    FxNavigate(FxNavigateTarget),
    AllTrackFxEnable(AllTrackFxEnableTarget),
    Transport(TransportTarget),
    AnyOn(AnyOnTarget),
    LoadFxSnapshot(LoadFxSnapshotTarget),
    AutomationTouchState(AutomationTouchStateTarget),
    GoToBookmark(GoToBookmarkTarget),
    Seek(SeekTarget),
    SendMidi(MidiSendTarget),
    SendOsc(OscSendTarget),
    ClipTransport(ClipTransportTarget),
    ClipSeek(ClipSeekTarget),
    ClipVolume(ClipVolumeTarget),
    LoadMappingSnapshot(LoadMappingSnapshotTarget),
    EnableMappings(EnableMappingsTarget),
    EnableInstances(EnableInstancesTarget),
    NavigateWithinGroup(NavigateWithinGroupTarget),
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
pub enum SendMidiDestination {
    #[serde(rename = "fx-output")]
    #[display(fmt = "FX output (with FX input only)")]
    FxOutput,
    #[serde(rename = "feedback-output")]
    #[display(fmt = "Feedback output")]
    FeedbackOutput,
}

impl Default for SendMidiDestination {
    fn default() -> Self {
        Self::FxOutput
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeekOptions {
    #[serde(default, skip_serializing_if = "is_default")]
    pub use_time_selection: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub use_loop_points: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub use_regions: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub use_project: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub move_view: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub seek_play: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub feedback_resolution: FeedbackResolution,
}

/// Determines in which granularity the play position influences feedback of a target.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
// Don't change the numbers! They are important for ordering. Higher number means higher resolution.
pub enum FeedbackResolution {
    /// Query for feedback every beat that's played on the main timeline.
    #[serde(rename = "beat")]
    #[display(fmt = "Beat")]
    Beat = 0,
    /// Query for feedback as frequently as possible (main loop).
    #[serde(rename = "high")]
    #[display(fmt = "Fast")]
    High = 1,
}

impl Default for FeedbackResolution {
    fn default() -> Self {
        Self::Beat
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
pub enum FxDisplayType {
    #[serde(rename = "floating")]
    #[display(fmt = "Floating window")]
    FloatingWindow,
    #[serde(rename = "chain")]
    #[display(fmt = "FX chain (limited feedback)")]
    Chain,
}

impl Default for FxDisplayType {
    fn default() -> Self {
        Self::FloatingWindow
    }
}

impl ReaperTarget {
    /// Notifies about other events which can affect the resulting `ReaperTarget`.
    ///
    /// The resulting `ReaperTarget` doesn't change only if one of our the model properties changes.
    /// It can also change if a track is removed or FX focus changes. We don't include
    /// those in `changed()` because they are global in nature. If we listen to n targets,
    /// we don't want to listen to those global events n times. Just 1 time is enough!
    pub fn potential_static_change_events(
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        let rx = Global::control_surface_rx();
        rx
            // Considering fx_focused() as static event is okay as long as we don't have a target
            // which switches focus between different FX. As soon as we have that, we must treat
            // fx_focused() as a dynamic event, like track_selection_changed().
            .fx_focused()
            .map_to(())
            .merge(rx.project_switched().map_to(()))
            .merge(rx.track_added().map_to(()))
            .merge(rx.track_removed().map_to(()))
            .merge(rx.tracks_reordered().map_to(()))
            .merge(rx.track_name_changed().map_to(()))
            .merge(rx.fx_added().map_to(()))
            .merge(rx.fx_removed().map_to(()))
            .merge(rx.fx_reordered().map_to(()))
            .merge(rx.bookmarks_changed())
            .merge(rx.receive_count_changed().map_to(()))
            .merge(rx.track_send_count_changed().map_to(()))
            .merge(rx.hardware_output_send_count_changed().map_to(()))
    }

    pub fn is_potential_change_event(evt: &ChangeEvent) -> bool {
        use ChangeEvent::*;
        matches!(
            evt,
            FxFocused(_)
                | ProjectSwitched(_)
                | TrackAdded(_)
                | TrackRemoved(_)
                | TracksReordered(_)
                | TrackNameChanged(_)
                | FxAdded(_)
                | FxRemoved(_)
                | FxReordered(_)
                | BookmarksChanged(_)
                | ReceiveCountChanged(_)
                | TrackSendCountChanged(_)
                | HardwareOutputSendCountChanged(_)
                | TrackSelectedChanged(_)
        )
    }

    /// This contains all potential target-changing events which could also be fired by targets
    /// themselves. Be careful with those. Reentrancy very likely.
    ///
    /// Previously we always reacted on selection changes. But this naturally causes issues,
    /// which become most obvious with the "Selected track" target. If we resync all mappings
    /// whenever another track is selected, this happens very often while turning an encoder
    /// that navigates between tracks. This in turn renders throttling functionality
    /// useless (because with a resync all runtime mode state is gone). Plus, reentrancy
    /// issues will arise.
    pub fn potential_dynamic_change_events(
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        let rx = Global::control_surface_rx();
        rx.track_selected_changed().map_to(())
    }

    /// This is eventually going to replace Rx (touched method), at least for domain layer.
    // TODO-medium Unlike the Rx stuff, this doesn't yet contain "Action touch". At the moment
    //  this leads to "Last touched target" to not work with actions - which might even desirable
    //  and should only added as soon as we allow explicitly enabling/disabling target types for
    //  this. The 2nd effect is that actions are not available for global learning which could be
    //  improved.
    pub fn touched_from_change_event(evt: ChangeEvent) -> Option<ReaperTarget> {
        use ChangeEvent::*;
        use ReaperTarget::*;
        let target = match evt {
            TrackVolumeChanged(e) if e.touched => TrackVolume(TrackVolumeTarget { track: e.track }),
            TrackPanChanged(e) if e.touched => {
                if let AvailablePanValue::Complete(new_value) = e.new_value {
                    figure_out_touched_pan_component(e.track, e.old_value, new_value)
                } else {
                    // Shouldn't result in this if touched.
                    return None;
                }
            }
            TrackRouteVolumeChanged(e) if e.touched => {
                TrackRouteVolume(RouteVolumeTarget { route: e.route })
            }
            TrackRoutePanChanged(e) if e.touched => {
                TrackRoutePan(RoutePanTarget { route: e.route })
            }
            TrackArmChanged(e) => TrackArm(TrackArmTarget {
                track: e.track,
                exclusivity: Default::default(),
            }),
            TrackMuteChanged(e) if e.touched => TrackMute(TrackMuteTarget {
                track: e.track,
                exclusivity: Default::default(),
            }),
            TrackSoloChanged(e) => {
                // When we press the solo button of some track, REAPER actually sends many
                // change events, starting with the change event for the master track. This is
                // not cool for learning because we could only ever learn master-track solo,
                // which doesn't even make sense. So let's just filter it out.
                if e.track.is_master_track() {
                    return None;
                }
                TrackSolo(TrackSoloTarget {
                    track: e.track,
                    behavior: Default::default(),
                    exclusivity: Default::default(),
                })
            }
            TrackSelectedChanged(e) if e.new_value => {
                if track_sel_on_mouse_is_enabled() {
                    // If this REAPER preference is enabled, it's often a false positive so better
                    // we don't let this happen at all.
                    return None;
                }
                TrackSelection(TrackSelectionTarget {
                    track: e.track,
                    exclusivity: Default::default(),
                    scroll_arrange_view: false,
                    scroll_mixer: false,
                })
            }
            FxEnabledChanged(e) => FxEnable(FxEnableTarget { fx: e.fx }),
            FxParameterValueChanged(e) if e.touched => FxParameter(FxParameterTarget {
                param: e.parameter,
                poll_for_feedback: true,
            }),
            FxPresetChanged(e) => FxPreset(FxPresetTarget { fx: e.fx }),
            MasterTempoChanged(e) if e.touched => Tempo(TempoTarget {
                // TODO-low In future this might come from a certain project
                project: Reaper::get().current_project(),
            }),
            MasterPlayrateChanged(e) if e.touched => Playrate(PlayrateTarget {
                // TODO-low In future this might come from a certain project
                project: Reaper::get().current_project(),
            }),
            TrackAutomationModeChanged(e) => TrackAutomationMode(TrackAutomationModeTarget {
                track: e.track,
                exclusivity: Default::default(),
                mode: e.new_value,
            }),
            GlobalAutomationOverrideChanged(e) => {
                AutomationModeOverride(AutomationModeOverrideTarget {
                    mode_override: e.new_value,
                })
            }
            _ => return None,
        };
        Some(target)
    }

    // TODO-medium This is the last Rx trace we have in processing logic and we should replace it
    //  in favor of async/await or direct calls. Still used by local learning and "Filter target".
    pub fn touched() -> impl LocalObservable<'static, Item = Rc<ReaperTarget>, Err = ()> + 'static {
        use ReaperTarget::*;
        let reaper = Reaper::get();
        let csurf_rx = Global::control_surface_rx();
        let action_rx = Global::action_rx();
        observable::empty()
            .merge(csurf_rx.fx_parameter_touched().map(move |param| {
                FxParameter(FxParameterTarget {
                    param,
                    poll_for_feedback: true,
                })
                .into()
            }))
            .merge(
                csurf_rx
                    .fx_enabled_changed()
                    .map(move |fx| FxEnable(FxEnableTarget { fx }).into()),
            )
            .merge(
                csurf_rx
                    .fx_preset_changed()
                    .map(move |fx| FxPreset(FxPresetTarget { fx }).into()),
            )
            .merge(
                csurf_rx
                    .track_volume_touched()
                    .map(move |track| TrackVolume(TrackVolumeTarget { track }).into()),
            )
            .merge(csurf_rx.track_pan_touched().map(move |(track, old, new)| {
                figure_out_touched_pan_component(track, old, new).into()
            }))
            .merge(csurf_rx.track_arm_changed().map(move |track| {
                TrackArm(TrackArmTarget {
                    track,
                    exclusivity: Default::default(),
                })
                .into()
            }))
            .merge(
                csurf_rx
                    .track_selected_changed()
                    .filter(|(_, new_value)| {
                        // If this REAPER preference is enabled, it's often a false positive so
                        // better we don't let this happen at all.
                        *new_value && !track_sel_on_mouse_is_enabled()
                    })
                    .map(move |(track, _)| {
                        TrackSelection(TrackSelectionTarget {
                            track,
                            exclusivity: Default::default(),
                            scroll_arrange_view: false,
                            scroll_mixer: false,
                        })
                        .into()
                    }),
            )
            .merge(csurf_rx.track_mute_touched().map(move |track| {
                TrackMute(TrackMuteTarget {
                    track,
                    exclusivity: Default::default(),
                })
                .into()
            }))
            .merge(csurf_rx.track_automation_mode_changed().map(move |track| {
                let mode = track.automation_mode();
                TrackAutomationMode(TrackAutomationModeTarget {
                    track,
                    exclusivity: Default::default(),
                    mode,
                })
                .into()
            }))
            .merge(
                csurf_rx
                    .track_solo_changed()
                    // When we press the solo button of some track, REAPER actually sends many
                    // change events, starting with the change event for the master track. This is
                    // not cool for learning because we could only ever learn master-track solo,
                    // which doesn't even make sense. So let's just filter it out.
                    .filter(|track| !track.is_master_track())
                    .map(move |track| {
                        TrackSolo(TrackSoloTarget {
                            track,
                            behavior: Default::default(),
                            exclusivity: Default::default(),
                        })
                        .into()
                    }),
            )
            .merge(
                csurf_rx
                    .track_route_volume_touched()
                    .map(move |route| TrackRouteVolume(RouteVolumeTarget { route }).into()),
            )
            .merge(
                csurf_rx
                    .track_route_pan_touched()
                    .map(move |route| TrackRoutePan(RoutePanTarget { route }).into()),
            )
            .merge(
                action_rx
                    .action_invoked()
                    .map(move |action| determine_target_for_action((*action).clone()).into()),
            )
            .merge(
                csurf_rx
                    .master_tempo_touched()
                    // TODO-low In future this might come from a certain project
                    .map(move |_| {
                        Tempo(TempoTarget {
                            project: reaper.current_project(),
                        })
                        .into()
                    }),
            )
            .merge(
                csurf_rx
                    .master_playrate_touched()
                    // TODO-low In future this might come from a certain project
                    .map(move |_| {
                        Playrate(PlayrateTarget {
                            project: reaper.current_project(),
                        })
                        .into()
                    }),
            )
            .merge(csurf_rx.global_automation_override_changed().map(move |_| {
                AutomationModeOverride(AutomationModeOverrideTarget {
                    mode_override: Reaper::get().global_automation_override(),
                })
                .into()
            }))
    }
}

impl<'a> Target<'a> for ReaperTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: ControlContext) -> Option<AbsoluteValue> {
        use ReaperTarget::*;
        match self {
            SendOsc(t) => t.current_value(context),
            SendMidi(t) => t.current_value(()),
            TrackPeak(t) => t.current_value(context),
            Action(t) => t.current_value(context),
            FxParameter(t) => t.current_value(context),
            TrackVolume(t) => t.current_value(context),
            TrackTool(t) => t.current_value(context),
            TrackPan(t) => t.current_value(context),
            TrackWidth(t) => t.current_value(context),
            TrackArm(t) => t.current_value(context),
            TrackRouteVolume(t) => t.current_value(context),
            TrackSelection(t) => t.current_value(context),
            TrackMute(t) => t.current_value(context),
            TrackPhase(t) => t.current_value(context),
            TrackShow(t) => t.current_value(context),
            TrackSolo(t) => t.current_value(context),
            TrackAutomationMode(t) => t.current_value(context),
            TrackRoutePan(t) => t.current_value(context),
            TrackRouteMute(t) => t.current_value(context),
            TrackRoutePhase(t) => t.current_value(context),
            TrackRouteMono(t) => t.current_value(context),
            TrackRouteAutomationMode(t) => t.current_value(context),
            Tempo(t) => t.current_value(context),
            Playrate(t) => t.current_value(context),
            AutomationModeOverride(t) => t.current_value(context),
            FxEnable(t) => t.current_value(context),
            FxOpen(t) => t.current_value(context),
            // Discrete
            FxPreset(t) => t.current_value(context),
            LoadFxSnapshot(t) => t.current_value(context),
            // Discrete
            SelectedTrack(t) => t.current_value(context),
            // Discrete
            FxNavigate(t) => t.current_value(context),
            AllTrackFxEnable(t) => t.current_value(context),
            Transport(t) => t.current_value(context),
            AnyOn(t) => t.current_value(context),
            AutomationTouchState(t) => t.current_value(context),
            GoToBookmark(t) => t.current_value(context),
            Seek(t) => t.current_value(context),
            ClipTransport(t) => t.current_value(context),
            ClipSeek(t) => t.current_value(context),
            ClipVolume(t) => t.current_value(context),
            LoadMappingSnapshot(t) => t.current_value(context),
            EnableMappings(t) => t.current_value(context),
            EnableInstances(t) => t.current_value(context),
            NavigateWithinGroup(t) => t.current_value(context),
        }
    }

    fn control_type(&self, context: ControlContext) -> ControlType {
        self.control_type_and_character(context).0
    }
}
impl<'a> Target<'a> for RealTimeReaperTarget {
    type Context = ();

    fn current_value(&self, _: ()) -> Option<AbsoluteValue> {
        use RealTimeReaperTarget::*;
        match self {
            SendMidi(t) => t.current_value(()),
        }
    }

    fn control_type(&self, _: ()) -> ControlType {
        use RealTimeReaperTarget::*;
        match self {
            SendMidi(t) => t.control_type(()),
        }
    }
}

// Panics if called with repeat or record.
pub(crate) fn clip_play_state_unit_value(
    action: TransportAction,
    play_state: ClipPlayState,
) -> UnitValue {
    use TransportAction::*;
    match action {
        PlayStop | PlayPause | Stop | Pause => match action {
            PlayStop | PlayPause => play_state.feedback_value(),
            Stop => transport_is_enabled_unit_value(play_state == ClipPlayState::Stopped),
            Pause => transport_is_enabled_unit_value(play_state == ClipPlayState::Paused),
            _ => unreachable!(),
        },
        _ => panic!("wrong argument"),
    }
}

pub fn current_value_of_bookmark(
    project: Project,
    bookmark_type: BookmarkType,
    index: u32,
    pos: PositionInSeconds,
) -> UnitValue {
    let current_bookmark = project.current_bookmark_at(pos);
    let relevant_index = match bookmark_type {
        BookmarkType::Marker => current_bookmark.marker_index,
        BookmarkType::Region => current_bookmark.region_index,
    };
    let is_current = relevant_index == Some(index);
    convert_bool_to_unit_value(is_current)
}

/// Converts a number of possible values to a step size.
pub fn convert_count_to_step_size(count: u32) -> UnitValue {
    // Dividing 1.0 by n would divide the unit interval (0..=1) into n same-sized
    // sub intervals, which means we would have n + 1 possible values. We want to
    // represent just n values, so we need n - 1 same-sized sub intervals.
    if count == 0 || count == 1 {
        return UnitValue::MAX;
    }
    UnitValue::new(1.0 / (count - 1) as f64)
}

pub fn format_value_as_playback_speed_factor_without_unit(value: UnitValue) -> String {
    let play_rate = PlayRate::from_normalized_value(NormalizedPlayRate::new(value.get()));
    format_playback_speed(play_rate.playback_speed_factor().get())
}

fn format_playback_speed(speed: f64) -> String {
    format!("{:.4}", speed)
}

pub fn format_step_size_as_playback_speed_factor_without_unit(value: UnitValue) -> String {
    // 0.0 => 0.0x
    // 1.0 => 3.75x
    let speed_increment = value.get() * playback_speed_factor_span();
    format_playback_speed(speed_increment)
}

pub fn format_value_as_bpm_without_unit(value: UnitValue) -> String {
    let tempo = Tempo::from_normalized_value(value.get());
    format_bpm(tempo.bpm().get())
}

pub fn format_step_size_as_bpm_without_unit(value: UnitValue) -> String {
    // 0.0 => 0.0 bpm
    // 1.0 => 959.0 bpm
    let bpm_increment = value.get() * bpm_span();
    format_bpm(bpm_increment)
}

// Should be 959.0
pub fn bpm_span() -> f64 {
    Bpm::MAX.get() - Bpm::MIN.get()
}

fn format_bpm(bpm: f64) -> String {
    format!("{:.4}", bpm)
}

pub fn format_value_as_pan(value: UnitValue) -> String {
    Pan::from_normalized_value(value.get()).to_string()
}

pub fn format_value_as_on_off(value: UnitValue) -> &'static str {
    format_bool_as_on_off(!value.is_zero())
}

pub fn format_bool_as_on_off(value: bool) -> &'static str {
    if value {
        "On"
    } else {
        "Off"
    }
}

pub fn convert_unit_value_to_preset_index(fx: &Fx, value: UnitValue) -> Option<u32> {
    convert_unit_to_discrete_value_with_none(value, fx.preset_count().ok()?)
}

pub fn convert_unit_value_to_track_index(project: Project, value: UnitValue) -> Option<u32> {
    convert_unit_to_discrete_value_with_none(value, project.track_count())
}

pub fn convert_unit_value_to_fx_index(fx_chain: &FxChain, value: UnitValue) -> Option<u32> {
    convert_unit_to_discrete_value_with_none(value, fx_chain.fx_count())
}

fn convert_unit_to_discrete_value_with_none(value: UnitValue, count: u32) -> Option<u32> {
    // Example: <no preset> + 4 presets
    if value.is_zero() {
        // 0.00 => <no preset>
        None
    } else {
        // 0.25 => 0
        // 0.50 => 1
        // 0.75 => 2
        // 1.00 => 3

        // Example: value = 0.75
        let step_size = 1.0 / count as f64; // 0.25
        let zero_based_value = (value.get() - step_size).max(0.0); // 0.5
        Some((zero_based_value * count as f64).round() as u32) // 2
    }
}

pub fn convert_unit_to_discrete_value(value: UnitValue, count: u32) -> u32 {
    if count == 0 {
        return 0;
    }
    (value.get() * (count - 1) as f64).round() as u32
}

pub fn convert_discrete_to_unit_value(value: u32, count: u32) -> UnitValue {
    if count < 2 {
        return UnitValue::MIN;
    }
    UnitValue::new_clamped(value as f64 / (count - 1) as f64)
}

pub fn selected_track_unit_value(project: Project, index: Option<u32>) -> UnitValue {
    convert_discrete_to_unit_value_with_none(index, project.track_count())
}

pub fn shown_fx_unit_value(fx_chain: &FxChain, index: Option<u32>) -> UnitValue {
    convert_discrete_to_unit_value_with_none(index, fx_chain.fx_count())
}

pub fn fx_preset_unit_value(fx: &Fx, index: Option<u32>) -> UnitValue {
    convert_discrete_to_unit_value_with_none(index, fx.preset_count().unwrap_or(0))
}

fn convert_discrete_to_unit_value_with_none(value: Option<u32>, count: u32) -> UnitValue {
    // Example: <no preset> + 4 presets
    match value {
        // <no preset> => 0.00
        None => UnitValue::MIN,
        // 0 => 0.25
        // 1 => 0.50
        // 2 => 0.75
        // 3 => 1.00
        Some(i) => {
            if count == 0 {
                return UnitValue::MIN;
            }
            // Example: i = 2
            let zero_based_value = i as f64 / count as f64; // 0.5
            let step_size = 1.0 / count as f64; // 0.25
            let value = (zero_based_value + step_size).min(1.0); // 0.75
            UnitValue::new(value)
        }
    }
}

pub fn parse_value_from_pan(text: &str) -> Result<UnitValue, &'static str> {
    let pan: Pan = text.parse()?;
    pan.normalized_value().try_into()
}

pub fn parse_value_from_playback_speed_factor(text: &str) -> Result<UnitValue, &'static str> {
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let factor: PlaybackSpeedFactor = decimal.try_into().map_err(|_| "not in play rate range")?;
    PlayRate::from_playback_speed_factor(factor)
        .normalized_value()
        .get()
        .try_into()
}

pub fn parse_step_size_from_playback_speed_factor(text: &str) -> Result<UnitValue, &'static str> {
    // 0.0x => 0.0
    // 3.75x => 1.0
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let span = playback_speed_factor_span();
    if decimal < 0.0 || decimal > span {
        return Err("not in playback speed factor increment range");
    }
    Ok(UnitValue::new(decimal / span))
}

/// Should be 3.75
pub fn playback_speed_factor_span() -> f64 {
    PlaybackSpeedFactor::MAX.get() - PlaybackSpeedFactor::MIN.get()
}

pub fn parse_value_from_bpm(text: &str) -> Result<UnitValue, &'static str> {
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let bpm: Bpm = decimal.try_into().map_err(|_| "not in BPM range")?;
    Tempo::from_bpm(bpm).normalized_value().try_into()
}

pub fn parse_step_size_from_bpm(text: &str) -> Result<UnitValue, &'static str> {
    // 0.0 bpm => 0.0
    // 959.0 bpm => 1.0
    let decimal: f64 = text.parse().map_err(|_| "not a decimal value")?;
    let span = bpm_span();
    if decimal < 0.0 || decimal > span {
        return Err("not in BPM increment range");
    }
    Ok(UnitValue::new(decimal / span))
}

/// How to invoke an action target
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
pub enum ActionInvocationType {
    #[display(fmt = "Trigger")]
    Trigger = 0,
    #[display(fmt = "Absolute")]
    Absolute = 1,
    #[display(fmt = "Relative")]
    Relative = 2,
}

impl Default for ActionInvocationType {
    fn default() -> Self {
        ActionInvocationType::Absolute
    }
}

#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Debug,
    Serialize,
    Deserialize,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TransportAction {
    #[serde(rename = "playStop")]
    #[display(fmt = "Play/stop")]
    PlayStop,
    #[serde(rename = "playPause")]
    #[display(fmt = "Play/pause")]
    PlayPause,
    #[serde(rename = "stop")]
    #[display(fmt = "Stop")]
    Stop,
    #[serde(rename = "pause")]
    #[display(fmt = "Pause")]
    Pause,
    #[serde(rename = "record")]
    #[display(fmt = "Record")]
    Record,
    #[serde(rename = "repeat")]
    #[display(fmt = "Repeat")]
    Repeat,
}

impl Default for TransportAction {
    fn default() -> Self {
        TransportAction::PlayStop
    }
}

fn determine_target_for_action(action: Action) -> ReaperTarget {
    let project = Reaper::get().current_project();
    match action.command_id().get() {
        // Play button | stop button
        1007 | 1016 => ReaperTarget::Transport(TransportTarget {
            project,
            action: TransportAction::PlayStop,
        }),
        // Pause button
        1008 => ReaperTarget::Transport(TransportTarget {
            project,
            action: TransportAction::PlayPause,
        }),
        // Record button
        1013 => ReaperTarget::Transport(TransportTarget {
            project,
            action: TransportAction::Record,
        }),
        // Repeat button
        1068 => ReaperTarget::Transport(TransportTarget {
            project,
            action: TransportAction::Repeat,
        }),
        _ => ReaperTarget::Action(ActionTarget {
            action,
            invocation_type: ActionInvocationType::Trigger,
            project,
            track: None,
        }),
    }
}

pub trait PanExt {
    /// Returns the pan value. In case of dual-pan, returns the left pan value.
    fn main_pan(self) -> ReaperPanValue;
    fn width(self) -> Option<ReaperWidthValue>;
}

impl PanExt for reaper_medium::Pan {
    /// Returns the pan value. In case of dual-pan, returns the left pan value.
    fn main_pan(self) -> ReaperPanValue {
        use reaper_medium::Pan::*;
        match self {
            BalanceV1(p) => p,
            BalanceV4(p) => p,
            StereoPan { pan, .. } => pan,
            DualPan { left, .. } => left,
            Unknown(_) => ReaperPanValue::CENTER,
        }
    }

    fn width(self) -> Option<ReaperWidthValue> {
        if let reaper_medium::Pan::StereoPan { width, .. } = self {
            Some(width)
        } else {
            None
        }
    }
}

fn figure_out_touched_pan_component(
    track: Track,
    old: reaper_medium::Pan,
    new: reaper_medium::Pan,
) -> ReaperTarget {
    if old.width() != new.width() {
        ReaperTarget::TrackWidth(TrackWidthTarget { track })
    } else {
        ReaperTarget::TrackPan(TrackPanTarget { track })
    }
}

pub fn pan_unit_value(pan: Pan) -> UnitValue {
    UnitValue::new(pan.normalized_value())
}

pub fn width_unit_value(width: Width) -> UnitValue {
    UnitValue::new(width.normalized_value())
}

pub fn track_arm_unit_value(is_armed: bool) -> UnitValue {
    convert_bool_to_unit_value(is_armed)
}

pub fn track_selected_unit_value(is_selected: bool) -> UnitValue {
    convert_bool_to_unit_value(is_selected)
}

pub fn mute_unit_value(is_mute: bool) -> UnitValue {
    convert_bool_to_unit_value(is_mute)
}

pub fn touched_unit_value(is_touched: bool) -> UnitValue {
    convert_bool_to_unit_value(is_touched)
}

pub fn track_solo_unit_value(is_solo: bool) -> UnitValue {
    convert_bool_to_unit_value(is_solo)
}

pub fn automation_mode_unit_value(
    desired_automation_mode: AutomationMode,
    actual_automation_mode: AutomationMode,
) -> UnitValue {
    let is_on = desired_automation_mode == actual_automation_mode;
    convert_bool_to_unit_value(is_on)
}

pub fn global_automation_mode_override_unit_value(
    desired_mode_override: Option<GlobalAutomationModeOverride>,
    actual_mode_override: Option<GlobalAutomationModeOverride>,
) -> UnitValue {
    convert_bool_to_unit_value(actual_mode_override == desired_mode_override)
}

pub fn tempo_unit_value(tempo: Tempo) -> UnitValue {
    UnitValue::new(tempo.normalized_value())
}

pub fn playrate_unit_value(playrate: PlayRate) -> UnitValue {
    UnitValue::new(playrate.normalized_value().get())
}

pub fn fx_enable_unit_value(is_enabled: bool) -> UnitValue {
    convert_bool_to_unit_value(is_enabled)
}

pub fn all_track_fx_enable_unit_value(is_enabled: bool) -> UnitValue {
    convert_bool_to_unit_value(is_enabled)
}

pub fn transport_is_enabled_unit_value(is_enabled: bool) -> UnitValue {
    convert_bool_to_unit_value(is_enabled)
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
pub enum SoloBehavior {
    #[display(fmt = "Solo in place")]
    InPlace,
    #[display(fmt = "Solo (ignore routing)")]
    IgnoreRouting,
    #[display(fmt = "Use REAPER preference")]
    ReaperPreference,
}

impl Default for SoloBehavior {
    fn default() -> Self {
        // We could choose ReaperPreference as default but that would be a bit against ReaLearn's
        // initial idea of being the number one tool for very project-specific mappings.
        SoloBehavior::InPlace
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TouchedParameterType {
    Volume,
    Pan,
    Width,
}

impl Default for TouchedParameterType {
    fn default() -> Self {
        TouchedParameterType::Volume
    }
}

impl TouchedParameterType {
    pub fn try_from_reaper(
        reaper_type: reaper_medium::TouchedParameterType,
    ) -> Result<Self, &'static str> {
        use reaper_medium::TouchedParameterType::*;
        let res = match reaper_type {
            Volume => Self::Volume,
            Pan => Self::Pan,
            Width => Self::Width,
            Unknown(_) => return Err("unknown touch parameter type"),
        };
        Ok(res)
    }
}

/// Returns if "Mouse click on volume/pan faders and track buttons changes track selection"
/// is enabled in the REAPER preferences.
fn track_sel_on_mouse_is_enabled() -> bool {
    use once_cell::sync::Lazy;
    static IS_ENABLED: Lazy<bool> = Lazy::new(query_track_sel_on_mouse_is_enabled);
    *IS_ENABLED
}

fn query_track_sel_on_mouse_is_enabled() -> bool {
    if let Some(res) = Reaper::get()
        .medium_reaper()
        .get_config_var("trackselonmouse")
    {
        if res.size != 4 {
            // Shouldn't be.
            return false;
        }
        let ptr = res.value.as_ptr() as *const u32;
        let value = unsafe { *ptr };
        // The second flag corresponds to that setting.
        (value & 2) != 0
    } else {
        false
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum TrackExclusivity {
    #[display(fmt = "No")]
    NonExclusive,
    #[display(fmt = "Within project")]
    ExclusiveWithinProject,
    #[display(fmt = "Within folder")]
    ExclusiveWithinFolder,
    #[display(fmt = "Within project (on only)")]
    ExclusiveWithinProjectOnOnly,
    #[display(fmt = "Within folder (on only)")]
    ExclusiveWithinFolderOnOnly,
}

impl Default for TrackExclusivity {
    fn default() -> Self {
        TrackExclusivity::NonExclusive
    }
}

impl TrackExclusivity {
    pub fn is_on_only(self) -> bool {
        use TrackExclusivity::*;
        matches!(
            self,
            ExclusiveWithinProjectOnOnly | ExclusiveWithinFolderOnOnly
        )
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize_repr,
    Deserialize_repr,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum Exclusivity {
    #[display(fmt = "Non-exclusive")]
    NonExclusive,
    #[display(fmt = "Exclusive")]
    Exclusive,
    #[display(fmt = "Exclusive (on only)")]
    ExclusiveOnOnly,
}

impl Default for Exclusivity {
    fn default() -> Self {
        Exclusivity::NonExclusive
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    IntoEnumIterator,
    TryFromPrimitive,
    IntoPrimitive,
    Display,
)]
#[repr(usize)]
pub enum SimpleExclusivity {
    #[display(fmt = "Non-exclusive")]
    NonExclusive,
    #[display(fmt = "Exclusive")]
    Exclusive,
}

impl Default for SimpleExclusivity {
    fn default() -> Self {
        SimpleExclusivity::NonExclusive
    }
}

impl From<Exclusivity> for SimpleExclusivity {
    fn from(e: Exclusivity) -> Self {
        use Exclusivity::*;
        match e {
            NonExclusive => Self::NonExclusive,
            Exclusive | ExclusiveOnOnly => Self::Exclusive,
        }
    }
}

impl From<SimpleExclusivity> for Exclusivity {
    fn from(e: SimpleExclusivity) -> Self {
        use SimpleExclusivity::*;
        match e {
            NonExclusive => Self::NonExclusive,
            Exclusive => Self::Exclusive,
        }
    }
}

impl HierarchyEntryProvider for Project {
    type Entry = Track;

    fn find_entry_by_index(&self, index: u32) -> Option<Self::Entry> {
        // TODO-medium This could be made faster by separating between heavy-weight and
        //  light-weight tracks in reaper-rs.
        self.track_by_index(index)
    }

    fn entry_count(&self) -> u32 {
        self.track_count()
    }
}

impl HierarchyEntry for Track {
    fn folder_depth_change(&self) -> i32 {
        self.folder_depth_change()
    }
}

pub fn change_track_prop(
    track: &Track,
    exclusivity: TrackExclusivity,
    control_value: UnitValue,
    mut enable: impl FnMut(&Track),
    mut disable: impl FnMut(&Track),
) {
    if control_value.is_zero() {
        // Case: Switch off
        if !exclusivity.is_on_only() {
            // Enable property for other tracks
            handle_exclusivity(
                &track.project(),
                exclusivity,
                track.index(),
                track,
                |_, track| enable(track),
            );
        }
        // Disable property for this track
        disable(track);
    } else {
        // Case: Switch on
        // Disable property for other tracks
        handle_exclusivity(
            &track.project(),
            exclusivity,
            track.index(),
            track,
            |_, track| disable(track),
        );
        // Enable property for this track
        enable(track);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RealTimeReaperTarget {
    SendMidi(MidiSendTarget),
}

pub fn get_control_type_and_character_for_track_exclusivity(
    exclusivity: TrackExclusivity,
) -> (ControlType, TargetCharacter) {
    if exclusivity == TrackExclusivity::NonExclusive {
        (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
    } else {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Switch,
        )
    }
}
