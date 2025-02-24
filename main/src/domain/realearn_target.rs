use crate::domain::ui_util::{
    format_as_percentage_without_unit, format_raw_midi, log_output,
    parse_unit_value_from_percentage, OutputReason,
};
use crate::domain::{
    AdditionalEelTransformationInput, AdditionalFeedbackEvent, DomainEventHandler, Exclusivity,
    ExtendedProcessorContext, FeedbackAudioHookTask, FeedbackOutput, GroupId, InstanceId,
    InstanceStateChanged, MainMapping, MappingControlResult, MappingId, OrderedMappingMap,
    OscFeedbackTask, ProcessorContext, RealTimeReaperTarget, RealTimeSender, ReaperTarget,
    SharedInstanceState, Tag, TagScope, TargetCharacter, TrackExclusivity, ACTION_TARGET,
    ALL_TRACK_FX_ENABLE_TARGET, ANY_ON_TARGET, AUTOMATION_MODE_OVERRIDE_TARGET,
    AUTOMATION_TOUCH_STATE_TARGET, CLIP_SEEK_TARGET, CLIP_TRANSPORT_TARGET, CLIP_VOLUME_TARGET,
    ENABLE_INSTANCES_TARGET, ENABLE_MAPPINGS_TARGET, FX_ENABLE_TARGET, FX_NAVIGATE_TARGET,
    FX_OPEN_TARGET, FX_PARAMETER_TARGET, FX_PRESET_TARGET, GO_TO_BOOKMARK_TARGET,
    LOAD_FX_SNAPSHOT_TARGET, LOAD_MAPPING_SNAPSHOT_TARGET, MIDI_SEND_TARGET,
    NAVIGATE_WITHIN_GROUP_TARGET, OSC_SEND_TARGET, PLAYRATE_TARGET, ROUTE_AUTOMATION_MODE_TARGET,
    ROUTE_MONO_TARGET, ROUTE_MUTE_TARGET, ROUTE_PAN_TARGET, ROUTE_PHASE_TARGET,
    ROUTE_VOLUME_TARGET, SEEK_TARGET, SELECTED_TRACK_TARGET, TEMPO_TARGET, TRACK_ARM_TARGET,
    TRACK_AUTOMATION_MODE_TARGET, TRACK_MUTE_TARGET, TRACK_PAN_TARGET, TRACK_PEAK_TARGET,
    TRACK_PHASE_TARGET, TRACK_SELECTION_TARGET, TRACK_SHOW_TARGET, TRACK_SOLO_TARGET,
    TRACK_TOOL_TARGET, TRACK_VOLUME_TARGET, TRACK_WIDTH_TARGET, TRANSPORT_TARGET,
};
use enum_dispatch::enum_dispatch;
use enum_iterator::IntoEnumIterator;
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, NumericValue, PropValue, RawMidiEvent, RgbColor,
    TransformationInputProvider, UnitValue,
};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use reaper_high::{ChangeEvent, Fx, Project, Reaper, Track, TrackRoute};
use reaper_medium::{CommandId, MidiOutputDeviceId};
use serde_repr::*;
use std::collections::HashSet;
use std::convert::TryInto;
use std::fmt::{Debug, Display, Formatter};

#[enum_dispatch(ReaperTarget)]
pub trait RealearnTarget {
    // TODO-low Instead of taking the ControlContext as parameter in each method, we could also
    //  choose to implement RealearnTarget for a wrapper that contains the control context.
    //  We did this with ValueFormatter and ValueParser.
    fn character(&self, context: ControlContext) -> TargetCharacter {
        self.control_type_and_character(context).1
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType>;

    fn control_type_and_character(&self, context: ControlContext)
        -> (ControlType, TargetCharacter);

    fn open(&self, context: ControlContext) {
        let _ = context;
        if let Some(fx) = self.fx() {
            fx.show_in_floating_window();
            return;
        }
        if let Some(track) = self.track() {
            track.select_exclusively();
            // Scroll to track
            Reaper::get()
                .main_section()
                .action_by_command_id(CommandId::new(40913))
                .invoke_as_trigger(Some(track.project()));
        }
    }

    /// Parses the given text as a target value and returns it as unit value.
    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let _ = context;
        parse_unit_value_from_percentage(text)
    }

    /// Parses the given text as a target step size and returns it as unit value.
    fn parse_as_step_size(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let _ = context;
        parse_unit_value_from_percentage(text)
    }

    /// This converts the given normalized value to a discrete value.
    ///
    /// Used for displaying discrete target values in edit fields.
    /// Must be implemented for discrete targets only which don't support parsing according to
    /// `can_parse_values()`, e.g. FX preset. This target reports a step size. If we want to
    /// display an increment or a particular value in an edit field, we don't show normalized
    /// values of course but a discrete number, by using this function. Should be the reverse of
    /// `convert_discrete_value_to_unit_value()` because latter is used for parsing.
    ///
    /// In case the target wants increments, this takes 63 as the highest possible value.
    ///
    /// # Errors
    ///
    /// Returns an error if this target doesn't report a step size.
    fn convert_unit_value_to_discrete_value(
        &self,
        input: UnitValue,
        context: ControlContext,
    ) -> Result<u32, &'static str> {
        if self.control_type_and_character(context).0.is_relative() {
            // Relative MIDI controllers support a maximum of 63 steps.
            return Ok((input.get() * 63.0).round() as _);
        }
        let _ = input;
        Err("not supported")
    }

    /// Formats the given value without unit.
    ///
    /// Shown in the mapping panel text field for continuous targets.
    fn format_value_without_unit(&self, value: UnitValue, context: ControlContext) -> String {
        self.format_as_discrete_or_percentage(value, context)
    }

    /// Shown as default textual feedback.
    fn text_value(&self, context: ControlContext) -> Option<String> {
        let _ = context;
        None
    }

    /// Usable in textual feedback expressions as
    /// [`helgoboss_learn::target_prop_keys::NUMERIC_VALUE`]. Don't implement for on/off values!
    fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
        let _ = context;
        None
    }

    /// Formats the given step size without unit.
    fn format_step_size_without_unit(
        &self,
        step_size: UnitValue,
        context: ControlContext,
    ) -> String {
        self.format_as_discrete_or_percentage(step_size, context)
    }

    /// Reusable function
    // TODO-medium Never overwritten. Can be factored out!
    fn format_as_discrete_or_percentage(
        &self,
        value: UnitValue,
        context: ControlContext,
    ) -> String {
        if self.character(context) == TargetCharacter::Discrete {
            self.convert_unit_value_to_discrete_value(value, context)
                .map(|v| v.to_string())
                .unwrap_or_default()
        } else {
            format_as_percentage_without_unit(value)
        }
    }
    /// If this returns true, a value will not be printed (e.g. because it's already in the edit
    /// field).
    fn hide_formatted_value(&self, context: ControlContext) -> bool {
        let _ = context;
        false
    }

    /// If this returns true, a step size will not be printed (e.g. because it's already in the
    /// edit field).
    fn hide_formatted_step_size(&self, context: ControlContext) -> bool {
        let _ = context;
        false
    }

    /// For mapping panel.
    fn value_unit(&self, context: ControlContext) -> &'static str {
        if self.character(context) == TargetCharacter::Discrete {
            ""
        } else {
            "%"
        }
    }

    /// For textual feedback.
    fn numeric_value_unit(&self, context: ControlContext) -> &'static str {
        self.value_unit(context)
    }

    fn step_size_unit(&self, context: ControlContext) -> &'static str {
        if self.character(context) == TargetCharacter::Discrete {
            ""
        } else {
            "%"
        }
    }
    /// Formats the value completely (including a possible unit).
    fn format_value(&self, value: UnitValue, context: ControlContext) -> String {
        self.format_value_generic(value, context)
    }

    // TODO-medium Never overwritten. Can be factored out!
    fn format_value_generic(&self, value: UnitValue, context: ControlContext) -> String {
        format!(
            "{} {}",
            self.format_value_without_unit(value, context),
            self.value_unit(context)
        )
    }
    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        let (_, _) = (value, context);
        Err("not supported")
    }

    fn can_report_current_value(&self) -> bool {
        // We will quickly realize if not.
        true
    }

    fn is_available(&self, context: ControlContext) -> bool;

    fn project(&self) -> Option<Project> {
        None
    }
    fn track(&self) -> Option<&Track> {
        None
    }
    fn fx(&self) -> Option<&Fx> {
        None
    }
    fn route(&self) -> Option<&TrackRoute> {
        None
    }
    fn track_exclusivity(&self) -> Option<TrackExclusivity> {
        None
    }

    /// Whether the target supports automatic feedback in response to some events or polling.
    ///
    /// If the target supports automatic feedback, you are left with a choice:
    ///
    /// - a) Using polling (continuously poll the target value).
    /// - b) Setting this to `false`.
    ///
    /// Choose (a) if the target value is a real, global target value that also can affect
    /// other mappings. Polling is obviously not the optimal choice because of the performance
    /// drawback ... but at least multiple mappings can participate.
    ///
    /// Choose (b) is if the target value is not global but artificial, that is, attached to the
    /// mapping itself - and can therefore not have any effect on other mappings. This is also
    /// not the optimal choice because other mappings can't participate in the feedback value ...
    /// but at least it's fast.
    fn supports_automatic_feedback(&self) -> bool {
        // Usually yes. We will quickly realize if not.
        true
    }

    /// Might return the new value if changed but is not required to! If it doesn't and the consumer
    /// wants to know the new value, it should just query the current value of the target.
    ///
    /// Is called in any case (even if feedback not enabled). So we can use it for general-purpose
    /// change event reactions such as reacting to transport stop.
    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        context: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        let (_, _) = (evt, context);
        (false, None)
    }

    fn splinter_real_time_target(&self) -> Option<RealTimeReaperTarget> {
        None
    }

    /// Like `convert_unit_value_to_discrete_value()` but in the other direction.
    ///
    /// Used for parsing discrete values of discrete targets that can't do real parsing according to
    /// `can_parse_values()`.
    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        if self.control_type_and_character(context).0.is_relative() {
            return (value as f64 / 63.0).try_into();
        }
        let _ = value;
        Err("not supported")
    }

    fn parse_value_from_discrete_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.convert_discrete_value_to_unit_value(
            text.parse().map_err(|_| "not a discrete value")?,
            context,
        )
    }

    /// Returns a value for the given key if the target supports it.
    ///
    /// You don't need to implement the commonly supported prop values here! They will
    /// be handed out automatically as a fallback by the calling method in case you return `None`.
    //
    // Requiring owned strings here makes the API more pleasant and is probably not a big deal
    // performance-wise (feedback strings don't get large). If we want to optimize this in future,
    // don't use Cows. The only real performance win would be to use a writer API.
    // With Cows, we would still need to turn ReaperStr into owned String. With writer API,
    // we could just read borrowed ReaperStr as str and write into the result buffer. However, in
    // practice we don't often get borrowed strings from Reaper anyway.
    // TODO-low Use a formatter API instead of returning owned strings (for this to work, we also
    //  need to adjust the textual feedback expression parsing to take advantage of it).
    fn prop_value(&self, key: &str, context: ControlContext) -> Option<PropValue> {
        let _ = key;
        let _ = context;
        None
    }
}

#[derive(Copy, Clone)]
pub enum CompoundChangeEvent<'a> {
    Reaper(&'a ChangeEvent),
    Additional(&'a AdditionalFeedbackEvent),
    Instance(&'a InstanceStateChanged),
}

pub fn get_track_name(t: &Track) -> String {
    if let Some(n) = t.name() {
        if n.to_str().is_empty() {
            format!("Track {}", t.index().unwrap_or(0) + 1)
        } else {
            n.into_string()
        }
    } else {
        "<Master>".to_string()
    }
}

pub fn get_track_color(t: &Track) -> Option<RgbColor> {
    let reaper_medium::RgbColor { r, g, b } = t.custom_color()?;
    Some(RgbColor::new(r, g, b))
}

pub trait InstanceContainer: Debug {
    /// Returns activated tags if they don't correspond to the tags in the args.
    fn enable_instances(&self, args: EnableInstancesArgs) -> Option<HashSet<Tag>>;
}

pub struct EnableInstancesArgs<'a> {
    pub initiator_instance_id: InstanceId,
    /// `None` if monitoring FX.
    pub initiator_project: Option<Project>,
    pub scope: &'a TagScope,
    pub is_enable: bool,
    pub exclusivity: Exclusivity,
}

#[derive(Copy, Clone, Debug)]
pub struct ControlContext<'a> {
    pub feedback_audio_hook_task_sender: &'a RealTimeSender<FeedbackAudioHookTask>,
    pub osc_feedback_task_sender: &'a crossbeam_channel::Sender<OscFeedbackTask>,
    pub feedback_output: Option<FeedbackOutput>,
    pub instance_container: &'a dyn InstanceContainer,
    pub instance_state: &'a SharedInstanceState,
    pub instance_id: &'a InstanceId,
    pub output_logging_enabled: bool,
    pub processor_context: &'a ProcessorContext,
}

impl<'a> ControlContext<'a> {
    pub fn send_raw_midi(
        &self,
        reason: OutputReason,
        dev_id: MidiOutputDeviceId,
        events: Vec<RawMidiEvent>,
    ) {
        if self.output_logging_enabled {
            for e in &events {
                log_output(self.instance_id, reason, format_raw_midi(e.bytes()));
            }
        }
        let _ = self
            .feedback_audio_hook_task_sender
            .send(FeedbackAudioHookTask::SendMidi(dev_id, events))
            .unwrap();
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MappingControlContext<'a> {
    pub control_context: ControlContext<'a>,
    pub mapping_data: MappingData,
}

impl<'a> TransformationInputProvider<AdditionalEelTransformationInput>
    for MappingControlContext<'a>
{
    fn additional_input(&self) -> AdditionalEelTransformationInput {
        AdditionalEelTransformationInput {
            y_last: self
                .mapping_data
                .last_non_performance_target_value
                .map(|v| v.to_unit_value().get())
                .unwrap_or_default(),
        }
    }
}

impl<'a> From<MappingControlContext<'a>> for ControlContext<'a> {
    fn from(v: MappingControlContext<'a>) -> Self {
        v.control_context
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MappingData {
    pub mapping_id: MappingId,
    pub group_id: GroupId,
    pub last_non_performance_target_value: Option<AbsoluteValue>,
}

pub type HitInstructionReturnValue = Option<Box<dyn HitInstruction>>;

pub trait HitInstruction {
    fn execute(self: Box<Self>, context: HitInstructionContext) -> Vec<MappingControlResult>;
}

pub struct HitInstructionContext<'a> {
    /// All mappings in the relevant compartment.
    pub mappings: &'a mut OrderedMappingMap<MainMapping>,
    // TODO-medium This became part of ExtendedProcessorContext, so redundant (not just here BTW)
    pub control_context: ControlContext<'a>,
    pub domain_event_handler: &'a dyn DomainEventHandler,
    pub logger: &'a slog::Logger,
    pub processor_context: ExtendedProcessorContext<'a>,
}

/// Type of a target
///
/// Display implementation produces single-line medium-length names that are supposed to be shown
/// e.g. in the dropdown.
///
/// IMPORTANT: Don't change the numbers! They are serialized.
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
)]
#[repr(usize)]
pub enum ReaperTargetType {
    // Global targets
    LastTouched = 20,
    AutomationModeOverride = 26,

    // Project targets
    AnyOn = 43,
    Action = 0,
    Transport = 16,
    SelectedTrack = 14,
    Seek = 23,
    Playrate = 11,
    Tempo = 10,

    // Marker/region targets
    GoToBookmark = 22,

    // Track targets
    TrackArm = 5,
    AllTrackFxEnable = 15,
    TrackTool = 44,
    TrackMute = 7,
    TrackPeak = 34,
    TrackPhase = 39,
    TrackSelection = 6,
    TrackAutomationMode = 25,
    AutomationTouchState = 21,
    TrackPan = 4,
    TrackWidth = 17,
    TrackVolume = 2,
    TrackShow = 24,
    TrackSolo = 8,

    // FX chain targets
    FxNavigate = 28,
    // FX targets
    FxEnable = 12,
    LoadFxSnapshot = 19,
    FxPreset = 13,
    FxOpen = 27,
    FxParameter = 1,

    // Send targets
    TrackSendAutomationMode = 42,
    TrackSendMono = 41,
    TrackSendMute = 18,
    TrackSendPhase = 40,
    TrackSendPan = 9,
    TrackSendVolume = 3,

    // Clip targets
    ClipTransport = 31,
    ClipSeek = 32,
    ClipVolume = 33,

    // Misc
    SendMidi = 29,
    SendOsc = 30,

    // ReaLearn targets
    EnableInstances = 38,
    EnableMappings = 36,
    LoadMappingSnapshot = 35,
    NavigateWithinGroup = 37,
}

impl Display for ReaperTargetType {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(self.definition().name())
    }
}

impl Default for ReaperTargetType {
    fn default() -> Self {
        ReaperTargetType::FxParameter
    }
}

impl ReaperTargetType {
    pub fn from_target(target: &ReaperTarget) -> ReaperTargetType {
        target
            .reaper_target_type()
            .expect("a REAPER target should always return a REAPER target type")
    }

    pub fn supports_feedback_resolution(self) -> bool {
        use ReaperTargetType::*;
        matches!(self, Seek | ClipSeek)
    }

    pub fn supports_poll_for_feedback(self) -> bool {
        use ReaperTargetType::*;
        matches!(
            self,
            FxParameter
                | TrackSendMute
                | TrackSendPhase
                | TrackSendMono
                | TrackSendAutomationMode
                | AllTrackFxEnable
                | TrackShow
                | TrackPhase
        )
    }

    pub const fn definition(self) -> &'static TargetTypeDef {
        use ReaperTargetType::*;
        match self {
            LastTouched => &LAST_TOUCHED_TARGET,
            AutomationModeOverride => &AUTOMATION_MODE_OVERRIDE_TARGET,
            AnyOn => &ANY_ON_TARGET,
            Action => &ACTION_TARGET,
            Transport => &TRANSPORT_TARGET,
            SelectedTrack => &SELECTED_TRACK_TARGET,
            Seek => &SEEK_TARGET,
            Playrate => &PLAYRATE_TARGET,
            Tempo => &TEMPO_TARGET,
            GoToBookmark => &GO_TO_BOOKMARK_TARGET,
            TrackArm => &TRACK_ARM_TARGET,
            AllTrackFxEnable => &ALL_TRACK_FX_ENABLE_TARGET,
            TrackTool => &TRACK_TOOL_TARGET,
            TrackMute => &TRACK_MUTE_TARGET,
            TrackPeak => &TRACK_PEAK_TARGET,
            TrackPhase => &TRACK_PHASE_TARGET,
            TrackSelection => &TRACK_SELECTION_TARGET,
            TrackAutomationMode => &TRACK_AUTOMATION_MODE_TARGET,
            AutomationTouchState => &AUTOMATION_TOUCH_STATE_TARGET,
            TrackPan => &TRACK_PAN_TARGET,
            TrackWidth => &TRACK_WIDTH_TARGET,
            TrackVolume => &TRACK_VOLUME_TARGET,
            TrackShow => &TRACK_SHOW_TARGET,
            TrackSolo => &TRACK_SOLO_TARGET,
            FxNavigate => &FX_NAVIGATE_TARGET,
            FxEnable => &FX_ENABLE_TARGET,
            LoadFxSnapshot => &LOAD_FX_SNAPSHOT_TARGET,
            FxPreset => &FX_PRESET_TARGET,
            FxOpen => &FX_OPEN_TARGET,
            FxParameter => &FX_PARAMETER_TARGET,
            TrackSendAutomationMode => &ROUTE_AUTOMATION_MODE_TARGET,
            TrackSendMono => &ROUTE_MONO_TARGET,
            TrackSendMute => &ROUTE_MUTE_TARGET,
            TrackSendPhase => &ROUTE_PHASE_TARGET,
            TrackSendPan => &ROUTE_PAN_TARGET,
            TrackSendVolume => &ROUTE_VOLUME_TARGET,
            ClipTransport => &CLIP_TRANSPORT_TARGET,
            ClipSeek => &CLIP_SEEK_TARGET,
            ClipVolume => &CLIP_VOLUME_TARGET,
            SendMidi => &MIDI_SEND_TARGET,
            SendOsc => &OSC_SEND_TARGET,
            EnableInstances => &ENABLE_INSTANCES_TARGET,
            EnableMappings => &ENABLE_MAPPINGS_TARGET,
            LoadMappingSnapshot => &LOAD_MAPPING_SNAPSHOT_TARGET,
            NavigateWithinGroup => &NAVIGATE_WITHIN_GROUP_TARGET,
        }
    }

    pub fn supports_track(self) -> bool {
        self.definition().supports_track()
    }

    pub fn supports_track_must_be_selected(self) -> bool {
        self.definition().supports_track_must_be_selected()
    }

    pub fn supports_track_scrolling(self) -> bool {
        self.definition().supports_track_scrolling()
    }

    pub fn supports_slot(self) -> bool {
        self.definition().supports_slot()
    }

    pub fn supports_fx(self) -> bool {
        self.definition().supports_fx()
    }

    pub fn supports_tags(self) -> bool {
        self.definition().supports_tags()
    }

    pub fn supports_fx_chain(self) -> bool {
        self.definition().supports_fx_chain()
    }

    pub fn supports_fx_display_type(self) -> bool {
        self.definition().supports_fx_display_type()
    }

    pub fn supports_send(self) -> bool {
        self.definition().supports_send()
    }

    pub fn supports_track_exclusivity(self) -> bool {
        self.definition().supports_track_exclusivity()
    }

    pub fn supports_exclusivity(self) -> bool {
        self.definition().supports_exclusivity()
    }

    pub fn supports_control(&self) -> bool {
        self.definition().supports_control()
    }

    pub fn supports_feedback(&self) -> bool {
        self.definition().supports_feedback()
    }

    pub fn hint(&self) -> &'static str {
        self.definition().hint()
    }

    /// Produces a shorter name than the Display implementation.
    ///
    /// For example, it doesn't contain the leading context information.
    pub fn short_name(&self) -> &'static str {
        self.definition().short_name()
    }
}

pub struct TargetTypeDef {
    pub name: &'static str,
    pub short_name: &'static str,
    pub hint: &'static str,
    pub supports_track: bool,
    pub if_so_supports_track_must_be_selected: bool,
    pub supports_track_scrolling: bool,
    pub supports_slot: bool,
    pub supports_fx: bool,
    pub supports_fx_chain: bool,
    pub supports_fx_display_type: bool,
    pub supports_tags: bool,
    pub supports_send: bool,
    pub supports_track_exclusivity: bool,
    pub supports_exclusivity: bool,
    pub supports_poll_for_feedback: bool,
    pub supports_feedback_resolution: bool,
    pub supports_control: bool,
    pub supports_feedback: bool,
}

impl TargetTypeDef {
    pub const fn name(&self) -> &'static str {
        self.name
    }
    pub const fn short_name(&self) -> &'static str {
        self.short_name
    }
    pub const fn hint(&self) -> &'static str {
        self.hint
    }
    pub const fn supports_track(&self) -> bool {
        self.supports_track
    }
    pub const fn supports_track_must_be_selected(&self) -> bool {
        self.supports_track() && self.if_so_supports_track_must_be_selected
    }
    pub const fn supports_track_scrolling(&self) -> bool {
        self.supports_track_scrolling
    }
    pub const fn supports_slot(&self) -> bool {
        self.supports_slot
    }
    pub const fn supports_fx(&self) -> bool {
        self.supports_fx
    }
    pub const fn supports_fx_chain(&self) -> bool {
        self.supports_fx() || self.supports_fx_chain
    }
    pub const fn supports_fx_display_type(&self) -> bool {
        self.supports_fx_display_type
    }
    pub const fn supports_tags(&self) -> bool {
        self.supports_tags
    }
    pub const fn supports_send(&self) -> bool {
        self.supports_send
    }
    pub const fn supports_track_exclusivity(&self) -> bool {
        self.supports_track_exclusivity
    }
    pub const fn supports_exclusivity(&self) -> bool {
        self.supports_exclusivity
    }
    pub const fn supports_poll_for_feedback(&self) -> bool {
        self.supports_poll_for_feedback
    }
    pub const fn supports_feedback_resolution(&self) -> bool {
        self.supports_feedback_resolution
    }
    pub const fn supports_control(&self) -> bool {
        self.supports_control
    }
    pub const fn supports_feedback(&self) -> bool {
        self.supports_feedback
    }
}

pub const DEFAULT_TARGET: TargetTypeDef = TargetTypeDef {
    name: "",
    short_name: "",
    hint: "",
    supports_control: true,
    supports_feedback: true,
    supports_track: false,
    if_so_supports_track_must_be_selected: true,
    supports_track_scrolling: false,
    supports_slot: false,
    supports_fx: false,
    supports_fx_chain: false,
    supports_fx_display_type: false,
    supports_tags: false,
    supports_send: false,
    supports_track_exclusivity: false,
    supports_exclusivity: false,
    supports_poll_for_feedback: false,
    supports_feedback_resolution: false,
};

pub const AUTOMATIC_FEEDBACK_VIA_POLLING_ONLY: &str = "Automatic feedback via polling only";

pub const LAST_TOUCHED_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Global: Last touched",
    short_name: "Last touched",
    ..DEFAULT_TARGET
};
