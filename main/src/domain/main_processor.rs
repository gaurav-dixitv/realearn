use crate::domain::{
    aggregate_target_values, ActivationChange, AdditionalFeedbackEvent, BackboneState,
    ClipChangedEvent, CompoundChangeEvent, CompoundFeedbackValue, CompoundMappingSource,
    CompoundMappingSourceAddress, CompoundMappingTarget, ControlContext, ControlInput, ControlMode,
    DeviceFeedbackOutput, DomainEvent, DomainEventHandler, ExtendedProcessorContext,
    FeedbackAudioHookTask, FeedbackDestinations, FeedbackOutput, FeedbackRealTimeTask,
    FeedbackResolution, FeedbackSendBehavior, GroupId, HitInstructionContext, InstanceContainer,
    InstanceOrchestrationEvent, InstanceStateChanged, IoUpdatedEvent, MainMapping,
    MainSourceMessage, MappingActivationEffect, MappingCompartment, MappingControlResult,
    MappingId, MappingInfo, MessageCaptureEvent, MessageCaptureResult, MidiDestination,
    MidiScanResult, NormalRealTimeTask, OrderedMappingIdSet, OrderedMappingMap, OscDeviceId,
    OscFeedbackTask, OscScanResult, ProcessorContext, QualifiedMappingId, QualifiedSource,
    RealFeedbackValue, RealTimeSender, RealearnMonitoringFxParameterValueChangedEvent,
    ReaperMessage, ReaperTarget, SharedInstanceState, SmallAsciiString, SourceFeedbackValue,
    SourceReleasedEvent, SpecificCompoundFeedbackValue, TargetValueChangedEvent,
    UpdatedSingleMappingOnStateEvent, VirtualSourceValue, CLIP_SLOT_COUNT,
};
use derive_more::Display;
use enum_map::EnumMap;
use helgoboss_learn::{
    AbsoluteValue, ControlValue, GroupInteraction, MidiSourceValue, MinIsMaxBehavior,
    ModeControlOptions, RawMidiEvent, Target, BASE_EPSILON,
};
use std::borrow::Cow;
use std::cell::RefCell;

use crate::domain::ui_util::{
    format_incoming_midi_message, format_midi_source_value, format_osc_message, format_osc_packet,
    format_raw_midi, log_control_input, log_feedback_output, log_learn_input, log_lifecycle_output,
    log_target_output,
};
use ascii::{AsciiString, ToAsciiChar};
use helgoboss_midi::{ControlChange14BitMessage, ParameterNumberMessage, RawShortMessage};
use reaper_high::{ChangeEvent, Reaper};
use reaper_medium::ReaperNormalizedFxParamValue;
use rosc::{OscMessage, OscPacket, OscType};
use slog::{debug, trace};
use smallvec::SmallVec;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};

// This can be come pretty big when multiple track volumes are adjusted at once.
const FEEDBACK_TASK_QUEUE_SIZE: usize = 20_000;
const NORMAL_TASK_BULK_SIZE: usize = 32;
const FEEDBACK_TASK_BULK_SIZE: usize = 64;
const CONTROL_TASK_BULK_SIZE: usize = 32;
const PARAMETER_TASK_BULK_SIZE: usize = 32;

pub const PLUGIN_PARAMETER_COUNT: u32 = 200;
pub const COMPARTMENT_PARAMETER_COUNT: u32 = 100;
pub type ParameterArray = [f32; PLUGIN_PARAMETER_COUNT as usize];
pub type ParameterSlice = [f32];
pub const ZEROED_PLUGIN_PARAMETERS: ParameterArray = [0.0f32; PLUGIN_PARAMETER_COUNT as usize];

#[derive(Debug)]
pub struct MainProcessor<EH: DomainEventHandler> {
    basics: Basics<EH>,
    collections: Collections,
    /// Contains IDs of those mappings who need to be polled as frequently as possible.
    poll_control_mappings: EnumMap<MappingCompartment, OrderedMappingIdSet>,
}

#[derive(Debug)]
struct Basics<EH: DomainEventHandler> {
    instance_id: InstanceId,
    instance_container: &'static dyn InstanceContainer,
    logger: slog::Logger,
    // TODO-medium Now that we communicate the feedback output separately, we could limit the scope
    //  of its meaning to "instance enabled etc."
    feedback_is_globally_enabled: bool,
    event_handler: EH,
    context: ProcessorContext,
    control_mode: ControlMode,
    control_is_globally_enabled: bool,
    control_input: ControlInput,
    feedback_output: Option<FeedbackOutput>,
    instance_state: SharedInstanceState,
    input_logging_enabled: bool,
    output_logging_enabled: bool,
    channels: Channels,
    // Using RefCell in the processing layer is an exception. We do it here because we can't
    // safely make feedback processing mutable. I tried (see branch
    // "experiment/feedback-change-detection-mutable") but it the end it turned out to be impossible
    // because the reaper-rs control surface doesn't emit feedback-triggering events in a mutable
    // context. Rightfully so, because it's potentially reentrant!
    last_feedback_checksum_by_address:
        RefCell<HashMap<CompoundMappingSourceAddress, FeedbackChecksum>>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum FeedbackChecksum {
    MidiPlain(RawShortMessage),
    MidiParameterNumber(ParameterNumberMessage),
    MidiControlChange14Bit(ControlChange14BitMessage),
    // For OSC and raw MIDI
    Hashed(u64),
}

impl FeedbackChecksum {
    fn from_value(v: &SourceFeedbackValue) -> Self {
        use SourceFeedbackValue::*;
        match v {
            Midi(v) => Self::from_midi(v),
            Osc(v) => Self::from_osc(v),
        }
    }

    fn from_midi(v: &MidiSourceValue<RawShortMessage>) -> Self {
        use MidiSourceValue::*;
        match v {
            Plain(v) => FeedbackChecksum::MidiPlain(*v),
            ParameterNumber(v) => FeedbackChecksum::MidiParameterNumber(*v),
            ControlChange14Bit(v) => FeedbackChecksum::MidiControlChange14Bit(*v),
            Raw { events, .. } => {
                let mut hasher = twox_hash::XxHash64::default();
                events.hash(&mut hasher);
                FeedbackChecksum::Hashed(hasher.finish())
            }
            Tempo(_) | BorrowedSysEx(_) => unreachable!("never sent as feedback"),
        }
    }

    fn from_osc(v: &OscMessage) -> Self {
        let mut hasher = twox_hash::XxHash64::default();
        // OscMessage doesn't implement Hash, probably because it contains floating point numbers.
        // We don't care about floating point hash/equality issues because we just want a checksum
        // for comparing current feedback with last feedback.
        v.addr.hash(&mut hasher);
        for arg in &v.args {
            hash_osc_arg(arg, &mut hasher);
        }
        FeedbackChecksum::Hashed(hasher.finish())
    }
}

fn hash_osc_arg<H: Hasher>(arg: &OscType, hasher: &mut H) {
    use OscType::*;
    match arg {
        Int(v) => {
            (0, v).hash(hasher);
        }
        Float(v) => {
            (1, v.to_ne_bytes()).hash(hasher);
        }
        String(v) => {
            (2, v).hash(hasher);
        }
        Blob(v) => {
            (3, v).hash(hasher);
        }
        Time(v) => {
            (4, v).hash(hasher);
        }
        Long(v) => {
            (5, v).hash(hasher);
        }
        Double(v) => {
            (6, v.to_ne_bytes()).hash(hasher);
        }
        Char(v) => {
            (7, v).hash(hasher);
        }
        Color(v) => {
            (8, (v.red, v.green, v.red, v.alpha)).hash(hasher);
        }
        Midi(v) => {
            (9, (v.port, v.status, v.data1, v.data2)).hash(hasher);
        }
        Bool(v) => {
            (10, v).hash(hasher);
        }
        Array(v) => {
            11.hash(hasher);
            for a in &v.content {
                hash_osc_arg(a, hasher);
            }
        }
        Nil => {
            12.hash(hasher);
        }
        Inf => {
            13.hash(hasher);
        }
    }
}

#[derive(Debug)]
struct Collections {
    /// Contains mappings without virtual targets.
    mappings: EnumMap<MappingCompartment, OrderedMappingMap<MainMapping>>,
    /// Contains mappings with virtual targets.
    mappings_with_virtual_targets: OrderedMappingMap<MainMapping>,
    /// Contains IDs of those mappings which should be refreshed as soon as a target is touched.
    /// At the moment only "Last touched" targets.
    target_touch_dependent_mappings: EnumMap<MappingCompartment, OrderedMappingIdSet>,
    /// Contains IDs of those mappings whose feedback might change depending on the current beat.
    beat_dependent_feedback_mappings: EnumMap<MappingCompartment, OrderedMappingIdSet>,
    /// Contains IDs of those mappings whose feedback might change depending on the current milli.
    /// TODO-low The mappings in there are polled regularly (even if main timeline is not playing).
    ///  could be optimized. However, this is what makes the seek target work currently when
    ///  changing cursor position while stopped.
    milli_dependent_feedback_mappings: EnumMap<MappingCompartment, OrderedMappingIdSet>,
    parameters: ParameterArray,
    previous_target_values: EnumMap<MappingCompartment, HashMap<MappingId, AbsoluteValue>>,
}

#[derive(Debug)]
struct Channels {
    self_feedback_sender: crossbeam_channel::Sender<FeedbackMainTask>,
    self_normal_sender: crossbeam_channel::Sender<NormalMainTask>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
    normal_real_time_to_main_thread_task_receiver:
        crossbeam_channel::Receiver<NormalRealTimeToMainThreadTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackMainTask>,
    parameter_task_receiver: crossbeam_channel::Receiver<ParameterMainTask>,
    instance_feedback_event_receiver: crossbeam_channel::Receiver<InstanceStateChanged>,
    control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
    normal_real_time_task_sender: RealTimeSender<NormalRealTimeTask>,
    feedback_real_time_task_sender: RealTimeSender<FeedbackRealTimeTask>,
    feedback_audio_hook_task_sender: RealTimeSender<FeedbackAudioHookTask>,
    osc_feedback_task_sender: crossbeam_channel::Sender<OscFeedbackTask>,
    additional_feedback_event_sender: crossbeam_channel::Sender<AdditionalFeedbackEvent>,
    instance_orchestration_event_sender: crossbeam_channel::Sender<InstanceOrchestrationEvent>,
    integration_test_feedback_sender: Option<crossbeam_channel::Sender<SourceFeedbackValue>>,
}

impl<EH: DomainEventHandler> MainProcessor<EH> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instance_id: InstanceId,
        parent_logger: &slog::Logger,
        self_normal_sender: crossbeam_channel::Sender<NormalMainTask>,
        normal_task_receiver: crossbeam_channel::Receiver<NormalMainTask>,
        normal_real_time_to_main_thread_task_receiver: crossbeam_channel::Receiver<
            NormalRealTimeToMainThreadTask,
        >,
        parameter_task_receiver: crossbeam_channel::Receiver<ParameterMainTask>,
        control_task_receiver: crossbeam_channel::Receiver<ControlMainTask>,
        instance_feedback_event_receiver: crossbeam_channel::Receiver<InstanceStateChanged>,
        normal_real_time_task_sender: RealTimeSender<NormalRealTimeTask>,
        feedback_real_time_task_sender: RealTimeSender<FeedbackRealTimeTask>,
        feedback_audio_hook_task_sender: RealTimeSender<FeedbackAudioHookTask>,
        additional_feedback_event_sender: crossbeam_channel::Sender<AdditionalFeedbackEvent>,
        instance_orchestration_event_sender: crossbeam_channel::Sender<InstanceOrchestrationEvent>,
        osc_feedback_task_sender: crossbeam_channel::Sender<OscFeedbackTask>,
        event_handler: EH,
        context: ProcessorContext,
        instance_state: SharedInstanceState,
        instance_container: &'static dyn InstanceContainer,
    ) -> MainProcessor<EH> {
        let (self_feedback_sender, feedback_task_receiver) =
            crossbeam_channel::bounded(FEEDBACK_TASK_QUEUE_SIZE);
        let logger = parent_logger.new(slog::o!("struct" => "MainProcessor"));
        MainProcessor {
            basics: Basics {
                instance_id,
                logger: logger.clone(),
                feedback_is_globally_enabled: false,
                event_handler,
                context,
                control_mode: ControlMode::Controlling,
                control_is_globally_enabled: true,
                control_input: Default::default(),
                feedback_output: Default::default(),
                instance_state,
                instance_container,
                input_logging_enabled: false,
                output_logging_enabled: false,
                channels: Channels {
                    self_feedback_sender,
                    self_normal_sender,
                    normal_task_receiver,
                    normal_real_time_to_main_thread_task_receiver,
                    feedback_task_receiver,
                    parameter_task_receiver,
                    instance_feedback_event_receiver,
                    control_task_receiver,
                    normal_real_time_task_sender,
                    feedback_real_time_task_sender,
                    feedback_audio_hook_task_sender,
                    osc_feedback_task_sender,
                    additional_feedback_event_sender,
                    instance_orchestration_event_sender,
                    integration_test_feedback_sender: None,
                },
                last_feedback_checksum_by_address: Default::default(),
            },
            collections: Collections {
                mappings: Default::default(),
                mappings_with_virtual_targets: Default::default(),
                target_touch_dependent_mappings: Default::default(),
                beat_dependent_feedback_mappings: Default::default(),
                milli_dependent_feedback_mappings: Default::default(),
                parameters: ZEROED_PLUGIN_PARAMETERS,
                previous_target_values: Default::default(),
            },
            poll_control_mappings: Default::default(),
        }
    }

    pub fn instance_id(&self) -> &InstanceId {
        &self.basics.instance_id
    }

    /// This is the chance to take over a source from another instance (send our feedback).
    ///
    /// This is a very important principle when using multiple instances. It allows feedback to
    /// not be accidentally cleared while still guaranteeing that feedback for non-used control
    /// elements are cleared eventually - independently from the order of instance processing.
    pub fn maybe_takeover_source(&self, released_event: &SourceReleasedEvent) -> bool {
        if Some(released_event.feedback_output) != self.basics.feedback_output {
            // Difference feedback device. No source takeover of course.
            return false;
        }
        if let Some(mapping_with_source) = self.all_mappings().find(|m| {
            m.feedback_is_effectively_on()
                && m.source()
                    .has_same_feedback_address_as_value(&released_event.feedback_value)
        }) {
            if let Some(followed_mapping) = self.follow_maybe_virtual_mapping(mapping_with_source) {
                if self.basics.instance_feedback_is_effectively_enabled() {
                    debug!(
                        self.basics.logger,
                        "Taking over source {:?}...",
                        mapping_with_source.source()
                    );
                    // TODO-low Shouldn't we update the single mapping-on state here?
                    let feedback = followed_mapping.feedback(true, self.basics.control_context());
                    self.send_feedback(FeedbackReason::TakeOverSource, feedback);
                    true
                } else {
                    debug!(
                        self.basics.logger,
                        "No source takeover of {:?} because feedback effectively disabled",
                        mapping_with_source.source()
                    );
                    false
                }
            } else {
                false
            }
        } else {
            false
        }
    }

    /// We previously wanted to switch off lights for a source but gave other instances the chance
    /// to take over. This is called if no takeover happened and it's safe to really turn the lights
    /// off.
    pub fn finally_switch_off_source(
        &self,
        feedback_output: FeedbackOutput,
        feedback_value: SourceFeedbackValue,
    ) {
        debug!(
            self.basics.logger,
            "Finally switching off source with {:?}...", feedback_value
        );
        self.basics.send_direct_source_feedback(
            feedback_output,
            FeedbackReason::FinallySwitchOffSource,
            feedback_value,
            false,
        );
    }

    /// This should be regularly called by the control surface in normal mode.
    pub fn run_all(&mut self) {
        self.run_essential();
        self.run_control();
    }

    /// Processes control tasks coming from the real-time processor.
    ///
    /// This should *not* be called by the control surface when it's globally learning targets
    /// because we want to pause controlling in that case! Otherwise we could control targets and
    /// they would be learned although not touched via mouse, that's not good.
    fn run_control(&mut self) {
        // Collect control tasks (we do that in any case to not let get channels full).
        let control_tasks: SmallVec<[ControlMainTask; CONTROL_TASK_BULK_SIZE]> = self
            .basics
            .channels
            .control_task_receiver
            .try_iter()
            .take(CONTROL_TASK_BULK_SIZE)
            .collect();
        // It's possible that control is disabled because another instance cancels us. In that case
        // the RealTimeProcessor won't know about it and keeps sending MIDI. Stop it here!
        if !self.control_is_effectively_enabled() {
            return;
        }
        self.process_control_tasks(control_tasks.into_iter());
        self.poll_control();
    }

    fn process_control_tasks(&mut self, control_tasks: impl Iterator<Item = ControlMainTask>) {
        for task in control_tasks {
            use ControlMainTask::*;
            match task {
                Control {
                    compartment,
                    mapping_id,
                    value,
                    options,
                } => {
                    let _ = self.control(compartment, mapping_id, value, options);
                }
                LogControlInput {
                    value,
                    match_result,
                } => {
                    log_control_input(
                        self.instance_id(),
                        format!("{} ({})", format_midi_source_value(&value), match_result),
                    );
                }
                LogLearnInput { msg } => {
                    log_learn_input(self.instance_id(), format_incoming_midi_message(msg));
                }
                LogTargetOutput { event } => {
                    log_target_output(self.instance_id(), format_raw_midi(event.bytes()));
                }
            }
        }
    }

    fn poll_control(&mut self) {
        for compartment in MappingCompartment::enum_iter() {
            for id in self.poll_control_mappings[compartment].iter() {
                let (control_result, group_interaction) =
                    if let Some(m) = self.collections.mappings[compartment].get_mut(id) {
                        if !m.control_is_effectively_on() {
                            continue;
                        }
                        let control_context = self.basics.control_context();
                        let mut control_result = m.poll_control(
                            control_context,
                            &self.basics.logger,
                            ExtendedProcessorContext::new(
                                &self.basics.context,
                                &self.collections.parameters,
                                control_context,
                            ),
                        );
                        control_mapping_stage_two(
                            &self.basics,
                            &mut control_result,
                            m,
                            ManualFeedbackProcessing::On {
                                mappings_with_virtual_targets: &self
                                    .collections
                                    .mappings_with_virtual_targets,
                            },
                        );
                        (control_result, m.group_interaction())
                    } else {
                        continue;
                    };

                // We only do target-value based group interaction after polling
                // (makes sense because control-value based one has been done at control
                // time already).
                let needs_group_interaction = control_result.successful
                    && matches!(
                        group_interaction,
                        GroupInteraction::SameTargetValue | GroupInteraction::InverseTargetValue
                    );
                control_mapping_stage_three(
                    &self.basics,
                    &mut self.collections,
                    compartment,
                    control_result,
                    if needs_group_interaction {
                        GroupInteractionProcessing::On(GroupInteractionInput {
                            mapping_id: *id,
                            // Control value is not important because we only do target-value
                            // based group interaction.
                            control_value: ControlValue::AbsoluteContinuous(Default::default()),
                            group_interaction,
                        })
                    } else {
                        GroupInteractionProcessing::Off
                    },
                );
            }
        }
    }

    /// Processes incoming control messages from the real-time processor.
    fn control(
        &mut self,
        compartment: MappingCompartment,
        mapping_id: MappingId,
        control_value: ControlValue,
        options: ControlOptions,
    ) -> Result<(), &'static str> {
        // Resolving mappings with virtual targets is not necessary anymore. It has
        // been done in the real-time processor already.
        let (control_result, group_interaction) = {
            let m = self.collections.mappings[compartment]
                .get_mut(&mapping_id)
                .ok_or("mapping not found")?;
            // Most of the time, the main processor won't even receive a MIDI-triggered control
            // instruction from the real-time processor for a mapping for which control is disabled,
            // because the real-time processor doesn't process disabled mappings. But if control is
            // (temporarily) disabled because a target condition is (temporarily) not met (e.g.
            // "track must be selected") and the real-time processor doesn't yet know about it,
            // there might be a short amount of time where we still receive control statements. We
            // filter them here.
            if !m.control_is_effectively_on() {
                return Ok(());
            }
            let control_result = control_mapping_stage_one_and_two(
                &self.basics,
                &self.collections.parameters,
                m,
                control_value,
                options,
                ManualFeedbackProcessing::On {
                    mappings_with_virtual_targets: &self.collections.mappings_with_virtual_targets,
                },
            );
            (control_result, m.group_interaction())
        };
        control_mapping_stage_three(
            &self.basics,
            &mut self.collections,
            compartment,
            control_result,
            GroupInteractionProcessing::On(GroupInteractionInput {
                mapping_id,
                control_value,
                group_interaction,
            }),
        );
        Ok(())
    }

    /// This should be regularly called by the control surface, even during global target learning.
    pub fn run_essential(&mut self) {
        self.process_normal_tasks_from_real_time_processor();
        self.process_normal_tasks_from_session();
        self.process_parameter_tasks();
        self.process_feedback_tasks();
        self.poll_slots();
        self.process_instance_feedback_events();
        self.poll_for_feedback()
    }

    /// This goes through all mappings that returned "high" feedback resolution - which they do if
    /// there are no appropriate change events to listen to and therefore need feedback polling.
    #[allow(clippy::float_cmp)]
    fn poll_for_feedback(&mut self) {
        for compartment in MappingCompartment::enum_iter() {
            for mapping_id in self.collections.milli_dependent_feedback_mappings[compartment].iter()
            {
                if let Some(m) = self.collections.mappings[compartment].get(mapping_id) {
                    let previous_target_values = &mut self.collections.previous_target_values;
                    let control_context = self.basics.control_context();
                    self.basics
                        .process_feedback_related_reaper_event_for_mapping(
                            m,
                            &self.collections.mappings_with_virtual_targets,
                            &mut |m, t| {
                                if m.mode().feedback_props_in_use().is_empty() {
                                    // No feedback props are used, which means we have pure
                                    // numeric feedback (no textual feedback, no prop-based feedback
                                    // style settings).
                                    // Numeric feedback is always in percentages, so we can
                                    // safely block feedback already here if we encounter
                                    // duplicate target values. So check for duplicate feedback!
                                    // TODO-high-discrete Maybe not true anymore with discrete
                                    //  targets.
                                    let (affected, new_value) = if let Some(value) =
                                        t.current_value(control_context)
                                    {
                                        // Check if changed
                                        match previous_target_values[compartment].entry(*mapping_id)
                                        {
                                            Entry::Occupied(mut e) => {
                                                // We really want to resend if there's the slightest
                                                // difference. It's okay to have direct comparison
                                                // because we know the source of these two values is
                                                // the same.
                                                if e.get().to_unit_value().get()
                                                    == value.to_unit_value().get()
                                                {
                                                    // Value hasn't changed.
                                                    (false, None)
                                                } else {
                                                    // Value has changed.
                                                    e.insert(value);
                                                    (true, Some(value))
                                                }
                                            }
                                            Entry::Vacant(e) => {
                                                // No feedback sent yet for that milli-dependent mapping.
                                                e.insert(value);
                                                (true, Some(value))
                                            }
                                        }
                                    } else {
                                        // Couldn't determine feedback value.
                                        (false, None)
                                    };
                                    if affected {
                                        m.update_last_non_performance_target_value_if_appropriate(
                                            new_value,
                                        );
                                    }
                                    (affected, new_value)
                                } else {
                                    // We use feedback props. That either means we have numeric
                                    // feedback with some prop-based feedback style or we have
                                    // text feedback.
                                    //
                                    // Props can change even if the main target value doesn't
                                    // change!
                                    //
                                    // Also, text feedback is not necessarily based on percentages.
                                    // This means we can have the situation that in terms of
                                    // percentages (usually relevant for control direction), the
                                    // current value might be below 0% or above 100%, which would
                                    // let the percentage (unit value) stay the same. But the
                                    // text feedback might go beyond that interval, so we should
                                    // always update it! Example: Seek target with "Use project"
                                    // enabled.

                                    // We are now required to return the current target value.
                                    let new_value = t.current_value(control_context);
                                    (true, new_value)
                                }
                            },
                        );
                }
            }
        }
    }

    fn process_instance_feedback_events(&mut self) {
        for event in self
            .basics
            .channels
            .instance_feedback_event_receiver
            .try_iter()
            .take(FEEDBACK_TASK_BULK_SIZE)
        {
            self.process_feedback_related_reaper_event(|mapping, target| {
                mapping.process_change_event(
                    target,
                    CompoundChangeEvent::Instance(&event),
                    self.basics.control_context(),
                )
            });
        }
    }

    fn poll_slots(&mut self) {
        // TODO-medium This is polled on each main loop cycle. As soon as we have more than 8 slots,
        //  We should introduce a set that contains the currently filled or playing slot numbers
        //  iterate over them only instead of all slots.
        let mut instance_state = self.basics.instance_state.borrow_mut();
        for i in 0..CLIP_SLOT_COUNT {
            for event in instance_state.poll_slot(i).into_iter() {
                let is_position_change = matches!(&event, ClipChangedEvent::ClipPosition(_));
                let instance_event = InstanceStateChanged::Clip {
                    slot_index: i,
                    event,
                };
                if is_position_change {
                    // Position changed. This happens very frequently when a clip is playing.
                    // Mappings with slot seek targets are in the beat-dependent feedback
                    // mapping set, not in the milli-dependent one (because we don't want to
                    // query their feedback value more than once in one main loop cycle).
                    for compartment in MappingCompartment::enum_iter() {
                        for mapping_id in
                            self.collections.beat_dependent_feedback_mappings[compartment].iter()
                        {
                            if let Some(m) = self.collections.mappings[compartment].get(mapping_id)
                            {
                                self.process_feedback_related_reaper_event_for_mapping(
                                    m,
                                    &mut |m, target| {
                                        m.process_change_event(
                                            target,
                                            CompoundChangeEvent::Instance(&instance_event),
                                            self.basics.control_context(),
                                        )
                                    },
                                );
                            }
                        }
                    }
                } else {
                    // Other property of clip changed.
                    self.process_feedback_related_reaper_event(|mapping, target| {
                        mapping.process_change_event(
                            target,
                            CompoundChangeEvent::Instance(&instance_event),
                            self.basics.control_context(),
                        )
                    });
                }
            }
        }
    }

    fn process_feedback_tasks(&mut self) {
        let feedback_tasks: SmallVec<[FeedbackMainTask; FEEDBACK_TASK_BULK_SIZE]> = self
            .basics
            .channels
            .feedback_task_receiver
            .try_iter()
            .take(FEEDBACK_TASK_BULK_SIZE)
            .collect();
        for task in feedback_tasks {
            use FeedbackMainTask::*;
            match task {
                TargetTouched => self.process_target_touched_event(),
            }
        }
    }

    fn process_target_touched_event(&mut self) {
        // A target has been touched! We re-resolve all "Last touched" targets so they
        // now control the last touched target.
        for compartment in MappingCompartment::enum_iter() {
            for mapping_id in self.collections.target_touch_dependent_mappings[compartment].iter() {
                // Virtual targets are not candidates for "Last touched" so we don't
                // need to consider them here.
                let fb = if let Some(m) = self.collections.mappings[compartment].get_mut(mapping_id)
                {
                    // We don't need to track activation updates because this target
                    // is always on. Switching off is not necessary since the last
                    // touched target can never be "unset".
                    let control_context = self.basics.control_context();
                    m.refresh_target(
                        ExtendedProcessorContext::new(
                            &self.basics.context,
                            &self.collections.parameters,
                            control_context,
                        ),
                        control_context,
                    );
                    if m.has_reaper_target() && m.has_resolved_successfully() {
                        if m.feedback_is_effectively_on() {
                            // TODO-medium Is this executed too frequently and maybe
                            // even sends redundant feedback!?
                            m.feedback(true, control_context)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                self.send_feedback(FeedbackReason::Normal, fb);
            }
        }
    }

    fn process_parameter_tasks(&mut self) {
        let parameter_tasks: SmallVec<[ParameterMainTask; PARAMETER_TASK_BULK_SIZE]> = self
            .basics
            .channels
            .parameter_task_receiver
            .try_iter()
            .take(PARAMETER_TASK_BULK_SIZE)
            .collect();
        for task in parameter_tasks {
            use ParameterMainTask::*;
            match task {
                UpdateAllParameters(parameters) => {
                    self.update_all_parameters(parameters);
                }
                UpdateParameter { index, value } => self.update_single_parameter(index, value),
            }
        }
    }

    // https://github.com/rust-lang/rust-clippy/issues/6066
    #[allow(clippy::needless_collect)]
    fn update_single_parameter(&mut self, index: u32, value: f32) {
        debug!(
            self.basics.logger,
            "Updating parameter {} to {}...", index, value
        );
        // Work around REAPER's inability to notify about parameter changes in
        // monitoring FX by simulating the notification ourselves.
        // Then parameter learning and feedback works at least for
        // ReaLearn monitoring FX instances, which is especially
        // useful for conditional activation.
        if self.basics.context.is_on_monitoring_fx_chain() {
            let parameter = self
                .basics
                .context
                .containing_fx()
                .parameter_by_index(index);
            self.basics
                .channels
                .additional_feedback_event_sender
                .try_send(
                    AdditionalFeedbackEvent::RealearnMonitoringFxParameterValueChanged(
                        RealearnMonitoringFxParameterValueChangedEvent {
                            parameter,
                            new_value: ReaperNormalizedFxParamValue::new(value as _),
                        },
                    ),
                )
                .unwrap();
        }
        // Update own value (important to do first)
        let previous_value = self.collections.parameters[index as usize];
        self.collections.parameters[index as usize] = value;
        self.basics
            .event_handler
            .handle_event(DomainEvent::UpdatedParameter { index, value });
        // Mapping activation is supported for both compartments and target activation
        // might change also in non-virtual controller mappings due to dynamic targets.
        if let Some(compartment) = MappingCompartment::by_absolute_param_index(index) {
            let mut changed_mappings = HashSet::new();
            let mut unused_sources = self.currently_feedback_enabled_sources(compartment, true);
            // In order to avoid a mutable borrow of mappings and an immutable borrow of
            // parameters at the same time, we need to separate into READ activation
            // effects and WRITE activation updates.
            // 1. Mapping activation: Read
            let activation_effects: Vec<MappingActivationEffect> = self
                .all_mappings_in_compartment(compartment)
                .filter_map(|m| {
                    m.check_activation_effect(&self.collections.parameters, index, previous_value)
                })
                .collect();
            // 2. Mapping activation: Write
            let mapping_activation_updates: Vec<ActivationChange> = activation_effects
                .into_iter()
                .filter_map(|eff| {
                    changed_mappings.insert(eff.id);
                    let m = get_normal_or_virtual_target_mapping_mut(
                        &mut self.collections.mappings,
                        &mut self.collections.mappings_with_virtual_targets,
                        compartment,
                        eff.id,
                    )?;
                    m.update_activation_from_effect(eff)
                })
                .collect();
            // 3. Target refreshment and determine unused sources
            let mut target_activation_changes: Vec<ActivationChange> = vec![];
            for m in all_mappings_in_compartment_mut(
                &mut self.collections.mappings,
                &mut self.collections.mappings_with_virtual_targets,
                compartment,
            ) {
                if m.target_can_be_affected_by_parameters() {
                    let control_context = self.basics.control_context();
                    let context = ExtendedProcessorContext::new(
                        &self.basics.context,
                        &self.collections.parameters,
                        control_context,
                    );
                    let (target_has_changed, activation_change) =
                        m.refresh_target(context, control_context);
                    if target_has_changed || activation_change.is_some() {
                        changed_mappings.insert(m.id());
                    }
                    if let Some(c) = activation_change {
                        target_activation_changes.push(c);
                    }
                }
                if m.feedback_is_effectively_on() {
                    // Mark source as used
                    if let Some(addr) = m.source().extract_feedback_address() {
                        unused_sources.remove(&addr);
                    }
                }
            }
            self.process_mapping_updates_due_to_parameter_changes(
                compartment,
                mapping_activation_updates,
                target_activation_changes,
                unused_sources,
                changed_mappings.into_iter(),
            )
        }
    }

    fn update_all_parameters(&mut self, parameters: Box<ParameterArray>) {
        debug!(self.basics.logger, "Updating all parameters...");
        self.collections.parameters = *parameters;
        self.basics
            .event_handler
            .handle_event(DomainEvent::UpdatedAllParameters(parameters));
        for compartment in MappingCompartment::enum_iter() {
            let mut mapping_activation_changes: Vec<ActivationChange> = vec![];
            let mut target_activation_changes: Vec<ActivationChange> = vec![];
            let mut changed_mappings = vec![];
            let mut unused_sources = self.currently_feedback_enabled_sources(compartment, true);
            for m in all_mappings_in_compartment_mut(
                &mut self.collections.mappings,
                &mut self.collections.mappings_with_virtual_targets,
                compartment,
            ) {
                if m.activation_can_be_affected_by_parameters() {
                    if let Some(update) = m.update_activation(&self.collections.parameters) {
                        mapping_activation_changes.push(update);
                    }
                }
                if m.target_can_be_affected_by_parameters() {
                    let control_context = self.basics.control_context();
                    let context = ExtendedProcessorContext::new(
                        &self.basics.context,
                        &self.collections.parameters,
                        control_context,
                    );
                    let (has_changed, activation_change) =
                        m.refresh_target(context, control_context);
                    if has_changed || activation_change.is_some() {
                        changed_mappings.push(m.id())
                    }
                    if let Some(u) = activation_change {
                        target_activation_changes.push(u);
                    }
                }
                if m.feedback_is_effectively_on() {
                    // Mark source as used
                    if let Some(addr) = m.source().extract_feedback_address() {
                        unused_sources.remove(&addr);
                    }
                }
            }
            self.process_mapping_updates_due_to_parameter_changes(
                compartment,
                mapping_activation_changes,
                target_activation_changes,
                unused_sources,
                changed_mappings.into_iter(),
            );
        }
    }

    fn process_normal_tasks_from_session(&mut self) {
        // We could also iterate directly while keeping the receiver open. But that would (for
        // good reason) prevent us from calling other methods that mutably borrow
        // self. To at least avoid heap allocations, we use a smallvec.
        let normal_tasks: SmallVec<[NormalMainTask; NORMAL_TASK_BULK_SIZE]> = self
            .basics
            .channels
            .normal_task_receiver
            .try_iter()
            .take(NORMAL_TASK_BULK_SIZE)
            .collect();
        let normal_task_count = normal_tasks.len();
        for task in normal_tasks {
            use NormalMainTask::*;
            match task {
                UpdateSettings {
                    control_input,
                    feedback_output,
                    input_logging_enabled,
                    output_logging_enabled,
                } => {
                    self.update_settings(
                        control_input,
                        feedback_output,
                        input_logging_enabled,
                        output_logging_enabled,
                    );
                }
                UpdateAllMappings(compartment, mappings) => {
                    self.update_all_mappings(compartment, mappings);
                }
                NotifyRealearnInstanceStarted => {
                    self.process_reaper_message(&ReaperMessage::RealearnInstanceStarted);
                }
                HitTarget { id, value } => {
                    self.hit_target(id, value);
                }
                // This is sent on events such as track list change, FX focus etc.
                RefreshAllTargets => {
                    self.refresh_all_targets();
                }
                UpdateSingleMapping(compartment, mapping) => {
                    self.update_single_mapping(compartment, mapping);
                }
                UpdatePersistentMappingProcessingState { id, state } => {
                    self.update_persistent_mapping_processing_state(id, state);
                }
                SendAllFeedback => {
                    self.send_all_feedback();
                }
                LogDebugInfo => {
                    self.log_debug_info(normal_task_count);
                }
                LogMapping(compartment, mapping_id) => {
                    self.log_mapping(compartment, mapping_id);
                }
                UpdateFeedbackIsGloballyEnabled(is_enabled) => {
                    self.update_feedback_is_globally_enabled(is_enabled);
                }
                StartLearnSource {
                    allow_virtual_sources,
                    osc_arg_index_hint,
                } => {
                    debug!(self.basics.logger, "Start learning source");
                    self.basics.control_mode = ControlMode::LearningSource {
                        allow_virtual_sources,
                        osc_arg_index_hint,
                    };
                }
                DisableControl => {
                    debug!(self.basics.logger, "Disable control");
                    self.basics.control_mode = ControlMode::Disabled;
                }
                ReturnToControlMode => {
                    debug!(self.basics.logger, "Return to control mode");
                    self.basics.control_mode = ControlMode::Controlling;
                }
                UpdateControlIsGloballyEnabled(is_enabled) => {
                    self.basics.control_is_globally_enabled = is_enabled;
                    let event = IoUpdatedEvent {
                        ..self.basic_io_changed_event()
                    };
                    self.send_io_update(event).unwrap();
                }
                UseIntegrationTestFeedbackSender(sender) => {
                    self.basics.channels.integration_test_feedback_sender = Some(sender);
                }
            }
        }
    }

    fn update_feedback_is_globally_enabled(&mut self, is_enabled: bool) {
        debug!(
            self.basics.logger,
            "Updating feedback_is_globally_enabled to {}", is_enabled
        );
        self.basics.clear_last_feedback();
        self.basics.feedback_is_globally_enabled = is_enabled;
        if is_enabled {
            for compartment in MappingCompartment::enum_iter() {
                self.handle_feedback_after_having_updated_all_mappings(compartment, HashMap::new());
            }
        } else {
            // Clear it completely. Other instances that might take over maybe don't use
            // all control elements and we don't want to leave traces.
            self.clear_all_feedback_allowing_source_takeover();
        };
        let event = self.feedback_output_usage_might_have_changed_event();
        self.send_io_update(event).unwrap();
    }

    fn refresh_all_targets(&mut self) {
        debug!(self.basics.logger, "Refreshing all targets...");
        for compartment in MappingCompartment::enum_iter() {
            let mut activation_updates: Vec<ActivationChange> = vec![];
            let mut changed_mappings = vec![];
            let mut unused_sources = self.currently_feedback_enabled_sources(compartment, false);
            // Mappings with virtual targets don't have to be refreshed because virtual
            // targets are always active and never change depending on circumstances.
            for m in self.collections.mappings[compartment].values_mut() {
                let control_context = self.basics.control_context();
                let context = ExtendedProcessorContext::new(
                    &self.basics.context,
                    &self.collections.parameters,
                    control_context,
                );
                let (target_changed, activation_update) =
                    m.refresh_target(context, control_context);
                if target_changed || activation_update.is_some() {
                    changed_mappings.push(m.id());
                }
                if let Some(u) = activation_update {
                    activation_updates.push(u);
                }
                if m.feedback_is_effectively_on() {
                    // Mark source as used
                    if let Some(addr) = m.source().extract_feedback_address() {
                        unused_sources.remove(&addr);
                    }
                }
            }
            if !activation_updates.is_empty() {
                // In some cases like closing projects, it's possible that this will
                // fail because the real-time processor is
                // already gone. But it doesn't matter.
                let _ = self.basics.channels.normal_real_time_task_sender.send(
                    NormalRealTimeTask::UpdateTargetActivations(compartment, activation_updates),
                );
            }
            // Important to send IO event first ...
            self.notify_feedback_dev_usage_might_have_changed(compartment);
            self.handle_feedback_after_having_updated_particular_mappings(
                compartment,
                unused_sources,
                changed_mappings.into_iter(),
            );
        }
        self.update_on_mappings();
    }

    fn update_settings(
        &mut self,
        control_input: ControlInput,
        feedback_output: Option<FeedbackOutput>,
        input_logging_enabled: bool,
        output_logging_enabled: bool,
    ) {
        self.basics.clear_last_feedback();
        self.basics.input_logging_enabled = input_logging_enabled;
        self.basics.output_logging_enabled = output_logging_enabled;
        let released_event = self.io_released_event();
        self.basics.control_input = control_input;
        self.basics.feedback_output = feedback_output;
        let changed_event = self.feedback_output_usage_might_have_changed_event();
        self.send_io_update(released_event).unwrap();
        self.send_io_update(changed_event).unwrap();
    }

    fn update_all_mappings(
        &mut self,
        compartment: MappingCompartment,
        mut mappings: Vec<MainMapping>,
    ) {
        debug!(
            self.basics.logger,
            "Updating {} mappings in {}...",
            mappings.len(),
            compartment,
        );
        self.basics.clear_last_feedback();
        let mut mappings_by_group: HashMap<GroupId, Vec<MappingId>> = HashMap::new();
        let mut mapping_infos: HashMap<QualifiedMappingId, MappingInfo> = HashMap::new();
        let mut unused_sources = self.currently_feedback_enabled_sources(compartment, true);
        self.collections.target_touch_dependent_mappings[compartment].clear();
        self.collections.beat_dependent_feedback_mappings[compartment].clear();
        self.collections.milli_dependent_feedback_mappings[compartment].clear();
        self.collections.previous_target_values[compartment].clear();
        self.poll_control_mappings[compartment].clear();
        // Refresh and splinter real-time mappings
        let real_time_mappings = mappings
            .iter_mut()
            .map(|m| {
                mappings_by_group
                    .entry(m.group_id())
                    .or_default()
                    .push(m.id());
                mapping_infos.insert(m.qualified_id(), m.take_mapping_info());
                let control_context = self.basics.control_context();
                m.init_target_and_activation(
                    ExtendedProcessorContext::new(
                        &self.basics.context,
                        &self.collections.parameters,
                        control_context,
                    ),
                    control_context,
                );
                if m.feedback_is_effectively_on() {
                    // Mark source as used
                    if let Some(addr) = m.source().extract_feedback_address() {
                        unused_sources.remove(&addr);
                    }
                }
                if m.needs_refresh_when_target_touched() {
                    self.collections.target_touch_dependent_mappings[compartment].insert(m.id());
                }
                let feedback_resolution = m.feedback_resolution();
                if feedback_resolution == Some(FeedbackResolution::Beat) {
                    self.collections.beat_dependent_feedback_mappings[compartment].insert(m.id());
                }
                if feedback_resolution == Some(FeedbackResolution::High) {
                    self.collections.milli_dependent_feedback_mappings[compartment].insert(m.id());
                }
                if m.wants_to_be_polled_for_control() {
                    self.poll_control_mappings[compartment].insert(m.id());
                }
                m.splinter_real_time_mapping()
            })
            .collect();
        // Update instance state
        {
            let mut instance_state = self.basics.instance_state.borrow_mut();
            instance_state.set_mappings_by_group(compartment, mappings_by_group);
            instance_state.set_mapping_infos(mapping_infos);
        }
        // Put into hash map in order to quickly look up mappings by ID
        let mapping_tuples = mappings.into_iter().map(|m| (m.id(), m));
        if compartment == MappingCompartment::ControllerMappings {
            let (virtual_target_mappings, normal_mappings) =
                mapping_tuples.partition(|(_, m)| m.has_virtual_target());
            self.collections.mappings[compartment] = normal_mappings;
            self.collections.mappings_with_virtual_targets = virtual_target_mappings;
        } else {
            self.collections.mappings[compartment] = mapping_tuples.collect();
        }
        // Sync to real-time processor
        self.basics
            .channels
            .normal_real_time_task_sender
            .send(NormalRealTimeTask::UpdateAllMappings(
                compartment,
                real_time_mappings,
            ))
            .unwrap();
        // Important to send IO event first ...
        self.notify_feedback_dev_usage_might_have_changed(compartment);
        // ... and then mapping update. Otherwise, if this is an upper-floor instance
        // clearing all mappings, other instances won't see yet that they are actually
        // allowed to take over sources! Which might delay the reactivation of
        // lower-floor instances.
        self.handle_feedback_after_having_updated_all_mappings(compartment, unused_sources);
        self.update_on_mappings();
    }

    fn process_normal_tasks_from_real_time_processor(&mut self) {
        for task in self
            .basics
            .channels
            .normal_real_time_to_main_thread_task_receiver
            .try_iter()
            .take(NORMAL_TASK_BULK_SIZE)
        {
            use NormalRealTimeToMainThreadTask::*;
            match task {
                CaptureMidi {
                    scan_result,
                    allow_virtual_sources,
                } => {
                    let event = MessageCaptureEvent {
                        result: MessageCaptureResult::Midi(scan_result),
                        allow_virtual_sources,
                        osc_arg_index_hint: None,
                    };
                    self.basics
                        .event_handler
                        .handle_event(DomainEvent::CapturedIncomingMessage(event));
                }
                FullResyncToRealTimeProcessorPlease => {
                    // We cannot provide everything that the real-time processor needs so we need
                    // to delegate to the session in order to let it do the resync (could be
                    // changed by also holding unnecessary things but for now, why not taking the
                    // session detour).
                    self.basics
                        .event_handler
                        .handle_event(DomainEvent::FullResyncRequested);
                }
                LogLifecycleOutput { value } => {
                    log_lifecycle_output(
                        &self.basics.instance_id,
                        format_midi_source_value(&value),
                    );
                }
            }
        }
    }

    fn basic_io_changed_event(&self) -> IoUpdatedEvent {
        let active = self.collections.mappings[MappingCompartment::MainMappings]
            .values()
            .any(|m| m.is_effectively_on());
        IoUpdatedEvent {
            instance_id: self.basics.instance_id,
            control_input: self.basics.control_input.device_input(),
            control_input_used: self.basics.control_is_globally_enabled && active,
            feedback_output: self.basics.feedback_output.and_then(|o| o.device_output()),
            feedback_output_used: self.basics.feedback_is_globally_enabled && active,
            feedback_output_usage_might_have_changed: false,
        }
    }

    fn control_is_effectively_enabled(&self) -> bool {
        self.basics.control_is_globally_enabled
            && BackboneState::get()
                .control_is_allowed(self.instance_id(), self.basics.control_input)
    }

    fn io_released_event(&self) -> IoUpdatedEvent {
        IoUpdatedEvent {
            control_input_used: false,
            feedback_output_used: false,
            ..self.feedback_output_usage_might_have_changed_event()
        }
    }

    fn feedback_output_usage_might_have_changed_event(&self) -> IoUpdatedEvent {
        IoUpdatedEvent {
            feedback_output_usage_might_have_changed: true,
            ..self.basic_io_changed_event()
        }
    }

    fn notify_feedback_dev_usage_might_have_changed(&self, compartment: MappingCompartment) {
        // A device is only considered to be "in use" if there's at least one
        // *main* mapping. It doesn't depend on
        // controller mappings.
        if compartment == MappingCompartment::MainMappings {
            let event = self.feedback_output_usage_might_have_changed_event();
            debug!(
                self.basics.logger,
                "IO event. Feedback output used: {:?}", event.feedback_output_used
            );
            self.send_io_update(event).unwrap();
        }
    }

    fn send_io_update(
        &self,
        event: IoUpdatedEvent,
    ) -> Result<(), crossbeam_channel::TrySendError<InstanceOrchestrationEvent>> {
        self.basics
            .channels
            .instance_orchestration_event_sender
            .try_send(InstanceOrchestrationEvent::IoUpdated(event))
    }

    fn get_normal_or_virtual_target_mapping(
        &self,
        compartment: MappingCompartment,
        id: MappingId,
    ) -> Option<&MainMapping> {
        self.collections.mappings[compartment].get(&id).or(
            if compartment == MappingCompartment::ControllerMappings {
                self.collections.mappings_with_virtual_targets.get(&id)
            } else {
                None
            },
        )
    }

    fn get_normal_or_virtual_target_mapping_mut(
        &mut self,
        id: QualifiedMappingId,
    ) -> Option<&mut MainMapping> {
        self.collections.mappings[id.compartment]
            .get_mut(&id.id)
            .or(
                if id.compartment == MappingCompartment::ControllerMappings {
                    self.collections
                        .mappings_with_virtual_targets
                        .get_mut(&id.id)
                } else {
                    None
                },
            )
    }

    pub fn process_additional_feedback_event(&self, event: &AdditionalFeedbackEvent) {
        if let AdditionalFeedbackEvent::BeatChanged(_) = event {
            // This is fired very frequently so we don't want to iterate over all mappings,
            // just the ones that need to be notified for feedback or whatever.
            for compartment in MappingCompartment::enum_iter() {
                for mapping_id in
                    self.collections.beat_dependent_feedback_mappings[compartment].iter()
                {
                    if let Some(m) = self.collections.mappings[compartment].get(mapping_id) {
                        self.process_feedback_related_reaper_event_for_mapping(
                            m,
                            &mut |m, target| {
                                m.process_change_event(
                                    target,
                                    CompoundChangeEvent::Additional(event),
                                    self.basics.control_context(),
                                )
                            },
                        );
                    }
                }
            }
        } else {
            // Okay, not fired that frequently, we can iterate over all mappings.
            self.process_feedback_related_reaper_event(|mapping, target| {
                mapping.process_change_event(
                    target,
                    CompoundChangeEvent::Additional(event),
                    self.basics.control_context(),
                )
            });
        }
    }

    pub fn process_control_surface_change_event(&self, event: &ChangeEvent) {
        if ReaperTarget::is_potential_change_event(event) {
            // Handle dynamic target changes and target activation depending on REAPER state.
            //
            // Whenever anything changes that just affects the main processor targets, resync all
            // targets to the main processor. We don't want to resync to the real-time processor
            // just because another track has been selected. First, it would reset any source state
            // (e.g. short/long press timers). Second, it wouldn't change anything about the
            // sources. We also don't want to resync modes to the main processor. First,
            // it would reset any mode state (e.g. throttling data). Second, it would -
            // again - not result in any change. There are several global conditions
            // which affect whether feedback will be sent from a target or not. Similar
            // global conditions decide what exactly produces the feedback values (e.g.
            // when there's a target which uses <Selected track>, then a track selection
            // change changes the feedback value producer).

            // We don't have mutable access to self here (for good reentrancy reasons) so we
            // do the refresh in the next main loop cycle. This is what we always did, also when
            // this was still based on Rx!
            self.basics
                .channels
                .self_normal_sender
                .try_send(NormalMainTask::RefreshAllTargets)
                .unwrap();
        }
        self.process_feedback_related_reaper_event(|mapping, target| {
            mapping.process_change_event(
                target,
                CompoundChangeEvent::Reaper(event),
                self.basics.control_context(),
            )
        });
    }

    /// The given function should return if the current target value is affected by this change
    /// and the new value. We do this because querying the value *immediately*
    /// using the target's `current_value()` method will in some or even many (?) cases give us the
    /// old value - which can lead to confusing feedback! In the past we unknowingly worked around
    /// this by deferring the value query to the next main cycle, but now that we have the nice
    /// non-rx change detection technique, we can do it right here, feedback without delay and
    /// avoid a redundant query.
    fn process_feedback_related_reaper_event(
        &self,
        mut f: impl Fn(&MainMapping, &ReaperTarget) -> (bool, Option<AbsoluteValue>),
    ) {
        for compartment in MappingCompartment::enum_iter() {
            // Mappings with virtual targets don't need to be considered here because they don't
            // cause feedback themselves.
            for m in self.collections.mappings[compartment].values() {
                self.process_feedback_related_reaper_event_for_mapping(m, &mut f);
            }
        }
    }

    /// The given function f is NOW required to return the current target value.
    fn process_feedback_related_reaper_event_for_mapping(
        &self,
        m: &MainMapping,
        f: &mut impl FnMut(&MainMapping, &ReaperTarget) -> (bool, Option<AbsoluteValue>),
    ) {
        self.basics
            .process_feedback_related_reaper_event_for_mapping(
                m,
                &self.collections.mappings_with_virtual_targets,
                f,
            );
    }

    pub fn notify_target_touched(&self) {
        self.basics
            .channels
            .self_feedback_sender
            .try_send(FeedbackMainTask::TargetTouched)
            .unwrap();
    }

    pub fn receives_osc_from(&self, device_id: &OscDeviceId) -> bool {
        self.basics.control_input == ControlInput::Osc(*device_id)
    }

    pub fn process_reaper_message(&mut self, msg: &ReaperMessage) {
        if self.basics.control_mode != ControlMode::Controlling {
            return;
        }
        if self.basics.input_logging_enabled {
            log_control_input(&self.basics.instance_id, msg.to_string());
        }
        if !self.control_is_effectively_enabled() {
            return;
        }
        let msg = MainSourceMessage::Reaper(msg);
        let results = self
            .basics
            .process_controller_mappings_with_virtual_targets(
                &mut self.collections.mappings_with_virtual_targets,
                &mut self.collections.mappings[MappingCompartment::MainMappings],
                msg,
                &self.collections.parameters,
            );
        for r in results {
            control_mapping_stage_three(
                &self.basics,
                &mut self.collections,
                r.compartment,
                r.control_result,
                GroupInteractionProcessing::On(r.group_interaction_input),
            )
        }
        self.process_mappings_with_real_targets(msg);
    }

    pub fn process_incoming_osc_packet(&mut self, packet: &OscPacket) {
        if self.basics.input_logging_enabled {
            match self.basics.control_mode {
                ControlMode::Controlling => {
                    log_control_input(&self.basics.instance_id, format_osc_packet(packet));
                }
                ControlMode::LearningSource { .. } => {
                    log_learn_input(&self.basics.instance_id, format_osc_packet(packet));
                }
                ControlMode::Disabled => {}
            }
        }
        match packet {
            OscPacket::Message(msg) => self.process_incoming_osc_message(msg),
            OscPacket::Bundle(bundle) => {
                for p in bundle.content.iter() {
                    self.process_incoming_osc_packet(p);
                }
            }
        }
    }

    fn process_incoming_osc_message(&mut self, msg: &OscMessage) {
        match self.basics.control_mode {
            ControlMode::Controlling => {
                if self.control_is_effectively_enabled() {
                    let msg = MainSourceMessage::Osc(msg);
                    let results = self
                        .basics
                        .process_controller_mappings_with_virtual_targets(
                            &mut self.collections.mappings_with_virtual_targets,
                            &mut self.collections.mappings[MappingCompartment::MainMappings],
                            msg,
                            &self.collections.parameters,
                        );
                    for r in results {
                        control_mapping_stage_three(
                            &self.basics,
                            &mut self.collections,
                            r.compartment,
                            r.control_result,
                            GroupInteractionProcessing::On(r.group_interaction_input),
                        )
                    }
                    self.process_mappings_with_real_targets(msg);
                }
            }
            ControlMode::LearningSource {
                allow_virtual_sources,
                osc_arg_index_hint,
            } => {
                let scan_result = OscScanResult {
                    message: msg.clone(),
                    dev_id: None,
                };
                let event = MessageCaptureEvent {
                    result: MessageCaptureResult::Osc(scan_result),
                    allow_virtual_sources,
                    osc_arg_index_hint,
                };
                self.basics
                    .event_handler
                    .handle_event(DomainEvent::CapturedIncomingMessage(event));
            }
            ControlMode::Disabled => {}
        }
    }

    /// Controls mappings with real targets in *both* compartments.
    fn process_mappings_with_real_targets(&mut self, msg: MainSourceMessage) {
        for compartment in MappingCompartment::enum_iter() {
            let mut enforce_target_refresh = false;
            // Search for 958 to know why we use a for loop here instead of collect().
            let mut results = vec![];
            for m in self.collections.mappings[compartment]
                .values_mut()
                .filter(|m| m.control_is_effectively_on())
            {
                let control_value = if let Some(cv) = m.control(msg) {
                    cv
                } else {
                    continue;
                };
                let options = ControlOptions {
                    enforce_target_refresh,
                    ..Default::default()
                };
                let control_result = control_mapping_stage_one_and_two(
                    &self.basics,
                    &self.collections.parameters,
                    m,
                    control_value,
                    options,
                    ManualFeedbackProcessing::On {
                        mappings_with_virtual_targets: &self
                            .collections
                            .mappings_with_virtual_targets,
                    },
                );
                enforce_target_refresh = true;
                let extended_control_result = ExtendedMappingControlResult {
                    control_result,
                    compartment,
                    group_interaction_input: GroupInteractionInput {
                        mapping_id: m.id(),
                        group_interaction: m.group_interaction(),
                        control_value,
                    },
                };
                results.push(extended_control_result);
            }
            for r in results {
                control_mapping_stage_three(
                    &self.basics,
                    &mut self.collections,
                    r.compartment,
                    r.control_result,
                    GroupInteractionProcessing::On(r.group_interaction_input),
                )
            }
        }
    }

    fn process_mapping_updates_due_to_parameter_changes(
        &mut self,
        compartment: MappingCompartment,
        mapping_activation_updates: Vec<ActivationChange>,
        target_activation_updates: Vec<ActivationChange>,
        unused_sources: HashMap<CompoundMappingSourceAddress, QualifiedSource>,
        changed_mappings: impl Iterator<Item = MappingId>,
    ) {
        // Send feedback
        self.handle_feedback_after_having_updated_particular_mappings(
            compartment,
            unused_sources,
            changed_mappings,
        );
        // Communicate activation changes to real-time processor
        if !mapping_activation_updates.is_empty() {
            self.basics
                .channels
                .normal_real_time_task_sender
                .send(NormalRealTimeTask::UpdateMappingActivations(
                    compartment,
                    mapping_activation_updates,
                ))
                .unwrap();
        }
        if !target_activation_updates.is_empty() {
            self.basics
                .channels
                .normal_real_time_task_sender
                .send(NormalRealTimeTask::UpdateTargetActivations(
                    compartment,
                    target_activation_updates,
                ))
                .unwrap();
        }
        // Update on mappings
        self.update_on_mappings();
    }

    fn update_single_mapping_on_state(&self, id: QualifiedMappingId) {
        let is_on =
            if let Some(m) = self.get_normal_or_virtual_target_mapping(id.compartment, id.id) {
                m.is_effectively_on()
            } else {
                false
            };
        self.basics
            .event_handler
            .handle_event(DomainEvent::UpdatedSingleMappingOnState(
                UpdatedSingleMappingOnStateEvent { id, is_on },
            ));
    }

    fn update_on_mappings(&self) {
        let instance_is_enabled = self.control_is_effectively_enabled()
            && self.basics.instance_feedback_is_effectively_enabled();
        let on_mappings = if instance_is_enabled {
            self.all_mappings()
                .filter(|m| m.is_effectively_on())
                .map(MainMapping::qualified_id)
                .collect()
        } else {
            HashSet::new()
        };
        self.basics
            .event_handler
            .handle_event(DomainEvent::UpdatedOnMappings(on_mappings));
    }

    fn send_feedback(
        &self,
        reason: FeedbackReason,
        feedback_values: impl IntoIterator<Item = CompoundFeedbackValue>,
    ) {
        self.basics.send_feedback(
            &self.collections.mappings_with_virtual_targets,
            reason,
            feedback_values,
        );
    }

    fn all_mappings(&self) -> impl Iterator<Item = &MainMapping> {
        self.all_mappings_without_virtual_targets()
            .chain(self.collections.mappings_with_virtual_targets.values())
    }

    /// Includes virtual mappings if the controller mapping compartment is queried.
    fn all_mappings_in_compartment(
        &self,
        compartment: MappingCompartment,
    ) -> impl Iterator<Item = &MainMapping> {
        self.collections.mappings[compartment].values().chain(
            self.collections
                .mappings_with_virtual_targets
                .values()
                // Include virtual target mappings if we are talking about controller compartment.
                .filter(move |_| compartment == MappingCompartment::ControllerMappings),
        )
    }

    fn all_mappings_without_virtual_targets(&self) -> impl Iterator<Item = &MainMapping> {
        MappingCompartment::enum_iter()
            .map(move |compartment| self.collections.mappings[compartment].values())
            .flatten()
    }

    pub fn send_all_feedback(&self) {
        self.basics.clear_last_feedback();
        self.send_feedback(FeedbackReason::Normal, self.feedback_all());
    }

    fn feedback_all(&self) -> Vec<CompoundFeedbackValue> {
        // Virtual targets don't cause feedback themselves
        self.all_mappings_without_virtual_targets()
            .filter_map(|m| {
                if m.feedback_is_effectively_on() {
                    m.feedback(true, self.basics.control_context())
                } else {
                    None
                }
            })
            .collect()
    }

    fn feedback_particular_mappings(
        &self,
        compartment: MappingCompartment,
        mapping_ids: impl Iterator<Item = MappingId>,
    ) -> Vec<CompoundFeedbackValue> {
        mapping_ids
            .filter_map(|id| {
                let m = self.get_normal_or_virtual_target_mapping(compartment, id)?;
                if m.feedback_is_effectively_on() {
                    self.get_mapping_feedback_follow_virtual(m)
                } else {
                    None
                }
            })
            .collect()
    }

    fn feedback_all_in_compartment(
        &self,
        compartment: MappingCompartment,
    ) -> Vec<CompoundFeedbackValue> {
        self.all_mappings_in_compartment(compartment)
            .filter_map(|m| {
                if m.feedback_is_effectively_on() {
                    self.get_mapping_feedback_follow_virtual(m)
                } else {
                    None
                }
            })
            .collect()
    }

    fn get_mapping_feedback_follow_virtual(
        &self,
        m: &MainMapping,
    ) -> Option<CompoundFeedbackValue> {
        let followed_mapping = self.follow_maybe_virtual_mapping(m)?;
        followed_mapping.feedback(true, self.basics.control_context())
    }

    fn follow_maybe_virtual_mapping<'a>(&'a self, m: &'a MainMapping) -> Option<&'a MainMapping> {
        if let Some(control_element) = m.virtual_target_control_element() {
            self.collections.mappings[MappingCompartment::MainMappings]
                .values()
                .find(|m| {
                    m.virtual_source_control_element() == Some(control_element)
                        && m.feedback_is_effectively_on()
                })
        } else {
            Some(m)
        }
    }

    pub fn handle_change_of_some_upper_floor_instance(
        &self,
        feedback_output: DeviceFeedbackOutput,
    ) {
        self.update_on_mappings();
        if self
            .basics
            .feedback_output
            .and_then(FeedbackOutput::device_output)
            == Some(feedback_output)
        {
            if self.basics.instance_feedback_is_effectively_enabled() {
                debug!(self.basics.logger, "Reactivating instance...");
                // For this to really work reliably (eventual feedback consistency), it was
                // necessary to let the direct MIDI device feedback process in the global
                // *audio hook*, not in the real-time processor. Because there's only one audio
                // hook can guarantee a deterministic feedback send order.
                self.send_all_feedback();
            } else {
                debug!(self.basics.logger, "Cancelling instance...");
                self.send_feedback(FeedbackReason::SuspendInstance, self.feedback_all_zero());
            }
        }
    }

    /// When feedback gets globally disabled.
    fn clear_all_feedback_allowing_source_takeover(&self) {
        debug!(
            self.basics.logger,
            "Clearing all feedback allowing source takeover..."
        );
        self.send_feedback(
            FeedbackReason::ClearAllAllowingSourceTakeover,
            self.feedback_all_zero(),
        );
    }

    /// When main processor goes away for good.
    fn clear_all_feedback_preventing_source_takeover(&self) {
        debug!(
            self.basics.logger,
            "Clearing all feedback preventing source takeover..."
        );
        self.send_feedback(
            FeedbackReason::ClearAllPreventingSourceTakeover,
            self.feedback_all_zero(),
        );
    }

    fn feedback_all_zero(&self) -> Vec<CompoundFeedbackValue> {
        // Mappings with virtual targets should not be included here because they might not be in
        // use and therefore should not *directly* send zeros. However, they will receive zeros
        // if one of the main mappings with virtual sources are connected to them.
        self.all_mappings_without_virtual_targets()
            .filter(|m| m.feedback_is_effectively_on())
            .filter_map(|m| m.off_feedback())
            .collect()
    }

    fn currently_feedback_enabled_sources(
        &self,
        compartment: MappingCompartment,
        include_virtual: bool,
    ) -> HashMap<CompoundMappingSourceAddress, QualifiedSource> {
        if include_virtual {
            self.all_mappings_in_compartment(compartment)
                .filter(|m| m.feedback_is_effectively_on())
                .filter_map(|m| {
                    Some((m.source().extract_feedback_address()?, m.qualified_source()))
                })
                .collect()
        } else {
            self.collections.mappings[compartment]
                .values()
                .filter(|m| m.feedback_is_effectively_on())
                .filter_map(|m| {
                    Some((m.source().extract_feedback_address()?, m.qualified_source()))
                })
                .collect()
        }
    }

    fn handle_feedback_after_having_updated_all_mappings(
        &mut self,
        compartment: MappingCompartment,
        now_unused_sources: HashMap<CompoundMappingSourceAddress, QualifiedSource>,
    ) {
        self.send_off_feedback_for_unused_sources(now_unused_sources);
        self.send_feedback(
            FeedbackReason::Normal,
            self.feedback_all_in_compartment(compartment),
        );
    }

    fn handle_feedback_after_having_updated_particular_mappings(
        &mut self,
        compartment: MappingCompartment,
        now_unused_sources: HashMap<CompoundMappingSourceAddress, QualifiedSource>,
        mapping_ids: impl Iterator<Item = MappingId>,
    ) {
        self.send_off_feedback_for_unused_sources(now_unused_sources);
        self.send_feedback(
            FeedbackReason::Normal,
            self.feedback_particular_mappings(compartment, mapping_ids),
        );
    }

    /// Indicate via off feedback the sources which are not in use anymore.
    fn send_off_feedback_for_unused_sources(
        &self,
        now_unused_sources: HashMap<CompoundMappingSourceAddress, QualifiedSource>,
    ) {
        for s in now_unused_sources.into_values() {
            self.send_feedback(FeedbackReason::ClearUnusedSource, s.off_feedback());
        }
    }

    fn log_debug_info(&mut self, task_count: usize) {
        // Summary
        let msg = format!(
            "\n\
            # Main processor\n\
            \n\
            - State: {:?} \n\
            - Total main mapping count: {} \n\
            - Enabled main mapping count: {} \n\
            - Total non-virtual controller mapping count: {} \n\
            - Enabled non-virtual controller mapping count: {} \n\
            - Total virtual controller mapping count: {} \n\
            - Enabled virtual controller mapping count: {} \n\
            - Normal task count: {} \n\
            - Control task count: {} \n\
            - Feedback task count: {} \n\
            - Parameter values: {:?} \n\
            ",
            self.basics.control_mode,
            self.collections.mappings[MappingCompartment::MainMappings].len(),
            self.collections.mappings[MappingCompartment::MainMappings]
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
            self.collections.mappings[MappingCompartment::ControllerMappings].len(),
            self.collections.mappings[MappingCompartment::ControllerMappings]
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
            self.collections.mappings_with_virtual_targets.len(),
            self.collections
                .mappings_with_virtual_targets
                .values()
                .filter(|m| m.control_is_effectively_on() || m.feedback_is_effectively_on())
                .count(),
            task_count,
            self.basics.channels.control_task_receiver.len(),
            self.basics.channels.feedback_task_receiver.len(),
            self.collections.parameters,
        );
        Reaper::get().show_console_msg(msg);
        // Detailed
        trace!(
            self.basics.logger,
            "\n\
            # Main processor\n\
            \n\
            {:#?}
            ",
            self
        );
    }

    fn log_mapping(&self, compartment: MappingCompartment, mapping_id: MappingId) {
        // Summary
        let mapping = self
            .all_mappings_in_compartment(compartment)
            .find(|m| m.id() == mapping_id);
        let msg = format!(
            "\n\
            # Main processor\n\
            \n\
            Mapping with ID {}:\n\
            {:#?}
            ",
            mapping_id, mapping
        );
        Reaper::get().show_console_msg(msg);
    }

    fn update_single_mapping(
        &mut self,
        compartment: MappingCompartment,
        mut mapping: Box<MainMapping>,
    ) {
        debug!(
            self.basics.logger,
            "Updating single mapping {:?} in {}...",
            mapping.id(),
            compartment,
        );
        self.basics.clear_last_feedback();
        // Refresh
        let control_context = self.basics.control_context();
        mapping.init_target_and_activation(
            ExtendedProcessorContext::new(
                &self.basics.context,
                &self.collections.parameters,
                control_context,
            ),
            control_context,
        );
        // Sync to real-time processor
        self.basics
            .channels
            .normal_real_time_task_sender
            .send(NormalRealTimeTask::UpdateSingleMapping(
                compartment,
                Box::new(Some(mapping.splinter_real_time_mapping())),
            ))
            .unwrap();
        // Update and feedback
        let id = QualifiedMappingId::new(compartment, mapping.id());
        // Important to do this before calculating diff feedback (because we might have
        // a textual feedback expression that contains the mapping name property).
        self.basics
            .instance_state
            .borrow_mut()
            .update_mapping_info(id, mapping.take_mapping_info());
        let diff_feedback = self.calc_diff_feedback_complicated(
            self.get_normal_or_virtual_target_mapping(mapping.compartment(), mapping.id()),
            &mapping,
        );
        self.update_map_entries(compartment, *mapping);
        self.send_diff_feedback(diff_feedback);
        self.update_single_mapping_on_state(id);
    }

    fn update_persistent_mapping_processing_state(
        &mut self,
        id: QualifiedMappingId,
        state: PersistentMappingProcessingState,
    ) {
        debug!(
            self.basics.logger,
            "Updating persistent processing state of mapping {:?} in {}", id.id, id.compartment
        );
        // Sync to real-time processor
        self.basics
            .channels
            .normal_real_time_task_sender
            .send(NormalRealTimeTask::UpdatePersistentMappingProcessingState { id, state })
            .unwrap();
        // Update
        let (was_on_before, is_on_now) =
            if let Some(m) = self.get_normal_or_virtual_target_mapping_mut(id) {
                let was_on_before = m.feedback_is_effectively_on();
                m.update_persistent_processing_state(state);
                (was_on_before, m.feedback_is_effectively_on())
            } else {
                (false, false)
            };
        // Send feedback if necessary (right now we assume that changed processing state doesn't
        // change anything about the source or target, so we use a much more simple mechanism to
        // determine necessary diff feedback than when updating the complete mapping).
        if was_on_before != is_on_now {
            if let Some(m) = self.get_normal_or_virtual_target_mapping(id.compartment, id.id) {
                let fb = if is_on_now {
                    Fb::normal(self.get_mapping_feedback_follow_virtual(&*m))
                } else {
                    Fb::unused(m.off_feedback())
                };
                self.send_feedback(fb.0, fb.1);
            }
        }
        self.update_single_mapping_on_state(id);
    }

    /// Collect feedback (important to send later as soon as mappings updated).
    #[must_use]
    fn calc_diff_feedback_complicated(
        &self,
        previous_mapping: Option<&MainMapping>,
        mapping: &MainMapping,
    ) -> (Fb, Fb) {
        if let Some(previous_mapping) = previous_mapping {
            // An existing mapping is being overwritten.
            if previous_mapping.feedback_is_effectively_on() {
                // And its light is currently on.
                if mapping
                    .source()
                    .has_same_feedback_address_as_source(previous_mapping.source())
                {
                    // Source is the same.
                    if mapping.feedback_is_effectively_on() {
                        // Lights should still be on.
                        // Send new lights.
                        (
                            Fb::none(),
                            Fb::normal(self.get_mapping_feedback_follow_virtual(&*mapping)),
                        )
                    } else {
                        // Lights should now be off.
                        (Fb::unused(mapping.off_feedback()), Fb::none())
                    }
                } else {
                    // Source has changed.
                    // Switch previous source light off.
                    let fb1 = Fb::unused(previous_mapping.off_feedback());
                    let fb2 = if mapping.feedback_is_effectively_on() {
                        // Lights should be on. Send new lights.
                        Fb::normal(self.get_mapping_feedback_follow_virtual(&*mapping))
                    } else {
                        Fb::none()
                    };
                    (fb1, fb2)
                }
            } else {
                // Previous lights were off.
                if mapping.feedback_is_effectively_on() {
                    // Now should be on.
                    (
                        Fb::none(),
                        Fb::normal(self.get_mapping_feedback_follow_virtual(&*mapping)),
                    )
                } else {
                    // Still off.
                    (Fb::none(), Fb::none())
                }
            }
        } else {
            // This mapping is new.
            if mapping.feedback_is_effectively_on() {
                // Lights on.
                (
                    Fb::none(),
                    Fb::normal(self.get_mapping_feedback_follow_virtual(&*mapping)),
                )
            } else {
                // Lights off.
                (Fb::none(), Fb::none())
            }
        }
    }

    fn send_diff_feedback(&self, (fb1, fb2): (Fb, Fb)) {
        self.send_feedback(fb1.0, fb1.1);
        self.send_feedback(fb2.0, fb2.1);
    }

    fn update_map_entries(&mut self, compartment: MappingCompartment, mapping: MainMapping) {
        if mapping.needs_refresh_when_target_touched() {
            self.collections.target_touch_dependent_mappings[compartment].insert(mapping.id());
        } else {
            self.collections.target_touch_dependent_mappings[compartment]
                .shift_remove(&mapping.id());
        }
        let influence = mapping.feedback_resolution();
        if influence == Some(FeedbackResolution::Beat) {
            self.collections.beat_dependent_feedback_mappings[compartment].insert(mapping.id());
        } else {
            self.collections.beat_dependent_feedback_mappings[compartment]
                .shift_remove(&mapping.id());
        }
        if influence == Some(FeedbackResolution::High) {
            self.collections.milli_dependent_feedback_mappings[compartment].insert(mapping.id());
        } else {
            self.collections.milli_dependent_feedback_mappings[compartment]
                .shift_remove(&mapping.id());
            self.collections.previous_target_values[compartment].remove(&mapping.id());
        }
        if mapping.wants_to_be_polled_for_control() {
            self.poll_control_mappings[compartment].insert(mapping.id());
        } else {
            self.poll_control_mappings[compartment].shift_remove(&mapping.id());
        }
        let relevant_map = if mapping.has_virtual_target() {
            self.collections.mappings[compartment].shift_remove(&mapping.id());
            &mut self.collections.mappings_with_virtual_targets
        } else {
            self.collections
                .mappings_with_virtual_targets
                .shift_remove(&mapping.id());
            &mut self.collections.mappings[compartment]
        };
        relevant_map.insert(mapping.id(), mapping);
    }

    fn hit_target(&mut self, id: QualifiedMappingId, value: AbsoluteValue) {
        let control_result = if let Some(m) =
            self.collections.mappings[id.compartment].get_mut(&id.id)
        {
            let control_context = self.basics.control_context();
            let mut control_result = m.control_from_target_directly(
                control_context,
                &self.basics.logger,
                ExtendedProcessorContext::new(
                    &self.basics.context,
                    &self.collections.parameters,
                    control_context,
                ),
                value,
            );
            control_mapping_stage_two(
                &self.basics,
                &mut control_result,
                m,
                ManualFeedbackProcessing::On {
                    mappings_with_virtual_targets: &self.collections.mappings_with_virtual_targets,
                },
            );
            Some(control_result)
        } else {
            None
        };
        if let Some(control_result) = control_result {
            control_mapping_stage_three(
                &self.basics,
                &mut self.collections,
                id.compartment,
                control_result,
                GroupInteractionProcessing::Off,
            );
        }
    }
}

/// State that contains only those properties of a mapping which ...
///
/// - make a difference in terms of processing
/// - are changed in response to processing
/// - and are persisted as part of the session.
///
/// These properties follow an unusual data flow, but still an unidirectional one: They are
/// propagated from the processing layer to the session (via synchronous event), persisted into the
/// session and sent back (asynchronously via channel) to the processor - which causes the actual
/// change.  
#[derive(Copy, Clone, Debug)]
pub struct PersistentMappingProcessingState {
    pub is_enabled: bool,
}

/// A task which is sent from time to time.
#[derive(Debug)]
pub enum NormalMainTask {
    /// Clears all mappings and uses the passed ones.
    UpdateAllMappings(MappingCompartment, Vec<MainMapping>),
    /// Replaces the given mapping.
    // Boxed because much larger struct size than other variants.
    UpdateSingleMapping(MappingCompartment, Box<MainMapping>),
    // Available separately for performance reasons, because these updates are also triggered
    // triggered by processing itself, so it should happen fast.
    UpdatePersistentMappingProcessingState {
        id: QualifiedMappingId,
        state: PersistentMappingProcessingState,
    },
    /// Invokes the "ReaLearn instance started" source.
    NotifyRealearnInstanceStarted,
    /// Instructs the main processor to hit the target directly.
    ///
    /// This doesn't invoke group interaction because it's meant to totally skip the mode.
    HitTarget {
        id: QualifiedMappingId,
        value: AbsoluteValue,
    },
    RefreshAllTargets,
    UpdateSettings {
        control_input: ControlInput,
        feedback_output: Option<FeedbackOutput>,
        input_logging_enabled: bool,
        output_logging_enabled: bool,
    },
    UpdateControlIsGloballyEnabled(bool),
    UpdateFeedbackIsGloballyEnabled(bool),
    SendAllFeedback,
    LogDebugInfo,
    LogMapping(MappingCompartment, MappingId),
    StartLearnSource {
        allow_virtual_sources: bool,
        osc_arg_index_hint: Option<u32>,
    },
    DisableControl,
    ReturnToControlMode,
    UseIntegrationTestFeedbackSender(crossbeam_channel::Sender<SourceFeedbackValue>),
}

/// A task which is sent from time to time from real-time to main processor.
#[derive(Debug)]
pub enum NormalRealTimeToMainThreadTask {
    CaptureMidi {
        scan_result: MidiScanResult,
        allow_virtual_sources: bool,
    },
    /// This is sent by the real-time processor after it has not been called for a while because
    /// the audio device was closed. It wants everything resynced:
    ///
    /// - All mappings
    /// - Instance settings
    /// - Feedback
    FullResyncToRealTimeProcessorPlease,
    LogLifecycleOutput {
        value: MidiSourceValue<'static, RawShortMessage>,
    },
}

/// A parameter-related task (which is potentially sent very frequently, just think of automation).
#[derive(Debug)]
pub enum ParameterMainTask {
    UpdateParameter { index: u32, value: f32 },
    UpdateAllParameters(Box<ParameterArray>),
}

/// A feedback-related task (which is potentially sent very frequently).
#[derive(Debug)]
pub enum FeedbackMainTask {
    /// Sent whenever a target has been touched (usually a subset of the value change events)
    /// and as a result the global "last touched target" has been updated.
    TargetTouched,
}

/// A control-related task (which is potentially sent very frequently).
pub enum ControlMainTask {
    Control {
        compartment: MappingCompartment,
        mapping_id: MappingId,
        value: ControlValue,
        options: ControlOptions,
    },
    LogControlInput {
        value: MidiSourceValue<'static, RawShortMessage>,
        match_result: InputMatchResult,
    },
    LogLearnInput {
        msg: OwnedIncomingMidiMessage,
    },
    LogTargetOutput {
        event: Box<RawMidiEvent>,
    },
}

pub enum OwnedIncomingMidiMessage {
    Short(RawShortMessage),
    SysEx(Vec<u8>),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct ControlOptions {
    pub enforce_send_feedback_after_control: bool,
    pub mode_control_options: ModeControlOptions,
    /// Set this flag if this control operation is part of processing multiple mappings within one
    /// transaction.
    /// Reason: Possibly triggered change events (e.g. change of selected track) will result in
    /// refreshing all targets *after* the transaction, which might be to late if the user relies on
    /// mapping order! Setting `refresh_target` will enforce refreshing (without updating cache).
    pub enforce_target_refresh: bool,
}

impl<EH: DomainEventHandler> Drop for MainProcessor<EH> {
    fn drop(&mut self) {
        debug!(self.basics.logger, "Dropping main processor...");
        if self.basics.instance_feedback_is_effectively_enabled() {
            // We clear feedback right here and now because that's the last chance.
            // Other instances can take over the feedback output afterwards.
            self.clear_all_feedback_preventing_source_takeover();
        }
        let _ = self.send_io_update(self.io_released_event());
    }
}

/// Different feedback reasons can but don't have to result in slightly different behavior.
///
/// In any case, they are nice for tracing when debugging feedback issues.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum FeedbackReason {
    /// When ReaLearn detects a single source as unused.
    ClearUnusedSource,
    /// When all feedback for that instance gets disabled (e.g. by instance deactivation) but other
    /// instances should get a chance to grab some sources. Must be processed even if feedback
    /// globally disabled (because sent *after* globally disabling feedback).
    ClearAllAllowingSourceTakeover,
    /// When all feedback for that instance gets disabled and switching off is more important than
    /// letting other instances take over (e.g. when removing instance completely). Only needs to
    /// be processed when feedback enabled.
    ClearAllPreventingSourceTakeover,
    /// When a lower-floor ReaLearn instance is cancelled by an upper-floor one. Must be processed
    /// even if feedback is effectively disabled (because sent at a time when the lower-floor
    /// instance is covered by the higher-floor instance already).
    SuspendInstance,
    /// Normal feedback scenarios.
    Normal,
    /// When a ReaLearn instance X takes control of a source after Y has released the source.
    TakeOverSource,
    /// When no ReaLearn instance has taken over a source and now it's finally being switched off.
    FinallySwitchOffSource,
}

impl FeedbackReason {
    /// When this returns true, it allows source takeover by other instances.
    pub fn is_source_release(self) -> bool {
        use FeedbackReason::*;
        matches!(
            self,
            ClearUnusedSource | ClearAllAllowingSourceTakeover | SuspendInstance
        )
    }

    /// When this returns true, the processor will initiate the feedback send logic (including
    /// source takeover) always regardless if feedback is globally enabled or not.
    pub fn is_always_allowed(self) -> bool {
        matches!(
            self,
            FeedbackReason::SuspendInstance | FeedbackReason::ClearAllAllowingSourceTakeover
        )
    }
}

impl<EH: DomainEventHandler> Basics<EH> {
    pub fn clear_last_feedback(&self) {
        self.last_feedback_checksum_by_address.borrow_mut().clear();
    }

    pub fn control_context(&self) -> ControlContext {
        ControlContext {
            feedback_audio_hook_task_sender: &self.channels.feedback_audio_hook_task_sender,
            osc_feedback_task_sender: &self.channels.osc_feedback_task_sender,
            feedback_output: self.feedback_output,
            instance_container: self.instance_container,
            instance_state: &self.instance_state,
            instance_id: &self.instance_id,
            output_logging_enabled: self.output_logging_enabled,
            processor_context: &self.context,
        }
    }

    pub fn process_group_interaction(
        &self,
        collections: &mut Collections,
        compartment: MappingCompartment,
        mapping_id: MappingId,
        control_value: ControlValue,
        control_was_successful: bool,
    ) {
        if let Some(m) = collections.mappings[compartment].get(&mapping_id) {
            // Group interaction
            let group_id = m.group_id();
            use GroupInteraction::*;
            match m.group_interaction() {
                None => {}
                SameControl | InverseControl => {
                    let control_value = if m.group_interaction().is_inverse() {
                        control_value.inverse()
                    } else {
                        control_value
                    };
                    self.process_other_mappings(
                        collections,
                        compartment,
                        mapping_id,
                        group_id,
                        |other_mapping, basics, parameters| {
                            let options = ControlOptions {
                                // Previous mappings in this transaction could affect
                                // subsequent mappings!
                                enforce_target_refresh: true,
                                ..Default::default()
                            };
                            control_mapping_stage_one_and_two(
                                basics,
                                parameters,
                                other_mapping,
                                control_value,
                                options,
                                ManualFeedbackProcessing::Off,
                            )
                        },
                    );
                }
                SameTargetValue | InverseTargetValue | InverseTargetValueOnOnly => {
                    if !control_was_successful {
                        return;
                    }
                    let context = self.control_context();
                    if let Some(reference_value) = m.current_aggregated_target_value(context) {
                        if m.group_interaction() == InverseTargetValueOnOnly
                            && !reference_value.is_on()
                        {
                            return;
                        }
                        let normalized_target_value = reference_value.normalize(
                            &m.mode().settings().target_value_interval,
                            &m.mode().settings().discrete_target_value_interval,
                            MinIsMaxBehavior::PreferOne,
                            m.mode().settings().use_discrete_processing,
                            BASE_EPSILON,
                        );
                        let inverse = m.group_interaction().is_inverse();
                        self.process_other_mappings(
                            collections,
                            compartment,
                            mapping_id,
                            group_id,
                            |other_mapping, basics, parameters| {
                                let control_context = basics.control_context();
                                other_mapping.control_from_target_via_group_interaction(
                                    normalized_target_value,
                                    ControlOptions {
                                        // Previous mappings in this transaction could affect
                                        // subsequent mappings!
                                        enforce_target_refresh: true,
                                        ..Default::default()
                                    },
                                    control_context,
                                    &basics.logger,
                                    inverse,
                                    ExtendedProcessorContext::new(
                                        &self.context,
                                        parameters,
                                        control_context,
                                    ),
                                )
                            },
                        );
                    }
                }
            }
        }
    }

    fn process_other_mappings(
        &self,
        collections: &mut Collections,
        compartment: MappingCompartment,
        mapping_id: MappingId,
        group_id: GroupId,
        f: impl Fn(&mut MainMapping, &Basics<EH>, &ParameterArray) -> MappingControlResult,
    ) {
        let other_mappings = collections.mappings[compartment]
            .values_mut()
            .filter(|other_m| {
                other_m.id() != mapping_id
                    && other_m.group_id() == group_id
                    && other_m.control_is_effectively_on()
            });
        // Interestingly, we can't use closures like for_each or filter_map here in the same way
        // (fails with mutable + immutable borrow error). So we use a for loop and fill the
        // result vector manually.
        // TODO-low Rust question 958: Figure out the difference to the for loop.
        let mut hit_instructions = vec![];
        for other_mapping in other_mappings {
            let other_control_result = f(other_mapping, self, &collections.parameters);
            if let Some(new_value) = other_control_result.new_target_value {
                self.notify_target_value_changed(other_mapping, new_value);
            }
            self.send_feedback(
                &collections.mappings_with_virtual_targets,
                FeedbackReason::Normal,
                other_control_result.feedback_value,
            );
            if let Some(hi) = other_control_result.hit_instruction {
                hit_instructions.push(hi);
            }
        }
        for hi in hit_instructions {
            hi.execute(HitInstructionContext {
                mappings: &mut collections.mappings[compartment],
                control_context: self.control_context(),
                domain_event_handler: &self.event_handler,
                logger: &self.logger,
                processor_context: ExtendedProcessorContext::new(
                    &self.context,
                    &collections.parameters,
                    self.control_context(),
                ),
            });
        }
    }

    /// The given function f is NOW required to return the current target value.
    // https://github.com/rust-lang/rust-clippy/issues/6066
    #[allow(clippy::needless_collect)]
    pub fn process_feedback_related_reaper_event_for_mapping(
        &self,
        m: &MainMapping,
        mappings_with_virtual_targets: &OrderedMappingMap<MainMapping>,
        f: &mut impl FnMut(&MainMapping, &ReaperTarget) -> (bool, Option<AbsoluteValue>),
    ) {
        // It's enough if one of the resolved targets is affected. Then we are going to need the
        // values of all of them!
        let mut at_least_one_target_is_affected = false;
        let new_values: Vec<Option<AbsoluteValue>> = m
            .targets()
            .iter()
            .filter_map(|target| {
                let target = match target {
                    CompoundMappingTarget::Reaper(t) => t,
                    _ => return None,
                };
                // Immediate value capturing. Makes OSC feedback *much* smoother in
                // combination with high-throughput thread. Especially quick pulls
                // of many faders at once profit from it because intermediate
                // values are captured and immediately sent so user doesn't see
                // stuttering faders on their device.
                // It's important to capture the current value from the event because
                // querying *at this time* from the target itself might result in
                // the old value to be returned. This is the case with FX parameter
                // changes for examples and especially in case of on/off targets this
                // can lead to horribly wrong feedback. Previously we didn't have this
                // issue because we always deferred to the next main loop cycle.
                let (value_changed, new_value) = f(m, target);
                if value_changed {
                    at_least_one_target_is_affected = true;
                }
                Some(new_value)
            })
            .collect();
        if !at_least_one_target_is_affected {
            return;
        }
        let new_target_value = aggregate_target_values(new_values.into_iter());
        if let Some(new_value) = new_target_value {
            // Feedback
            let mapping_feedback_is_effectively_on = m.feedback_is_effectively_on();
            let with_projection_feedback = mapping_feedback_is_effectively_on;
            let with_source_feedback = self.instance_feedback_is_effectively_enabled()
                && mapping_feedback_is_effectively_on;
            let feedback_value = m
                .feedback_entry_point(
                    with_projection_feedback,
                    with_source_feedback,
                    new_value,
                    self.control_context(),
                )
                .map(CompoundFeedbackValue::normal);
            self.send_feedback(
                mappings_with_virtual_targets,
                FeedbackReason::Normal,
                feedback_value,
            );
            self.notify_target_value_changed(m, new_value);
        }
    }

    /// Inform session, e.g. for UI updates
    fn notify_target_value_changed(&self, m: &MainMapping, new_value: AbsoluteValue) {
        self.event_handler
            .handle_event(DomainEvent::TargetValueChanged(TargetValueChangedEvent {
                compartment: m.compartment(),
                mapping_id: m.id(),
                targets: m.targets(),
                new_value,
            }));
    }

    /// Processes (controller) mappings with virtual targets.
    ///
    /// This also includes controlling the (main) mappings with corresponding virtual sources.
    #[must_use]
    pub fn process_controller_mappings_with_virtual_targets(
        &self,
        mappings_with_virtual_targets: &mut OrderedMappingMap<MainMapping>,
        // Contains mappings with virtual sources
        main_mappings: &mut OrderedMappingMap<MainMapping>,
        msg: MainSourceMessage,
        parameters: &ParameterArray,
    ) -> Vec<ExtendedMappingControlResult> {
        // Control
        let mut extended_control_results: Vec<_> = mappings_with_virtual_targets
            .values_mut()
            .filter(|m| m.control_is_effectively_on())
            .flat_map(|m| {
                if let Some(virtual_source_value) = m.control_virtualizing(msg) {
                    self.event_handler
                        .notify_mapping_matched(MappingCompartment::ControllerMappings, m.id());
                    self.process_main_mappings_with_virtual_sources(
                        main_mappings,
                        virtual_source_value,
                        ControlOptions {
                            // We inherit "Send feedback after control" if it's
                            // enabled for the virtual mapping. That's the easy way to do it.
                            // Downside: If multiple real control elements are mapped to one
                            // virtual control element,
                            // "feedback after control" will be sent to all of
                            // those, which is technically not
                            // necessary. It would be enough to just send it
                            // to the one that was touched. However, it also doesn't really
                            // hurt.
                            enforce_send_feedback_after_control: m.options().feedback_send_behavior
                                == FeedbackSendBehavior::SendFeedbackAfterControl,
                            mode_control_options: m.mode_control_options(),
                            // Not yet important at this point because one virtual target can't
                            // affect a subsequent one.
                            enforce_target_refresh: false,
                        },
                        parameters,
                    )
                } else {
                    vec![]
                }
            })
            .collect();
        // Feedback
        self.send_feedback(
            mappings_with_virtual_targets,
            FeedbackReason::Normal,
            extended_control_results
                .iter_mut()
                .filter_map(|r| r.control_result.feedback_value.take()),
        );
        extended_control_results
    }

    /// Sends both direct and virtual-source feedback.
    pub fn send_feedback(
        &self,
        mappings_with_virtual_targets: &OrderedMappingMap<MainMapping>,
        feedback_reason: FeedbackReason,
        feedback_values: impl IntoIterator<Item = CompoundFeedbackValue>,
    ) {
        for feedback_value in feedback_values.into_iter() {
            match feedback_value.value {
                SpecificCompoundFeedbackValue::Virtual {
                    destinations,
                    value,
                } => {
                    // At this point we still include controller mappings for which feedback
                    // is explicitly not enabled (not supported by controller) in order to
                    // support at least projection feedback (#414)!
                    // Iterate over (controller) mappings with virtual targets.
                    for m in mappings_with_virtual_targets.values() {
                        // Should always be true.
                        if let Some(t) = m.virtual_target() {
                            if t.control_element() == value.control_element() {
                                // Virtual source matched virtual target. The following method
                                // will always produce real target values (because controller
                                // mappings can't have virtual sources).
                                if let Some(SpecificCompoundFeedbackValue::Real(
                                    final_feedback_value,
                                )) = m.feedback_given_target_value(
                                    // This clone is unavoidable because we are producing
                                    // real feedback values and these will be sent to another
                                    //  thread, so they must be self-contained.
                                    Cow::Borrowed(value.feedback_value()),
                                    FeedbackDestinations {
                                        with_source_feedback: destinations.with_source_feedback
                                            && m.feedback_is_enabled(),
                                        ..destinations
                                    },
                                ) {
                                    // Successful virtual-to-real feedback
                                    self.send_direct_feedback(
                                        feedback_reason,
                                        final_feedback_value,
                                        feedback_value.is_feedback_after_control,
                                    );
                                }
                            }
                        }
                    }
                }
                SpecificCompoundFeedbackValue::Real(final_feedback_value) => {
                    self.send_direct_feedback(
                        feedback_reason,
                        final_feedback_value,
                        feedback_value.is_feedback_after_control,
                    );
                }
            }
        }
    }

    pub fn send_direct_source_feedback(
        &self,
        feedback_output: FeedbackOutput,
        feedback_reason: FeedbackReason,
        source_feedback_value: SourceFeedbackValue,
        is_feedback_after_control: bool,
    ) {
        // Block duplicates.
        // Extracting a feedback address is not super cheap for OSC and MIDI Raw because it has to
        // clone the address string. On the other hand, address strings are not large, so what.
        if let Some(address) = source_feedback_value.extract_address() {
            let checksum = FeedbackChecksum::from_value(&source_feedback_value);
            let previous_checksum = self
                .last_feedback_checksum_by_address
                .borrow_mut()
                .insert(address, checksum);
            if !is_feedback_after_control && Some(checksum) == previous_checksum {
                trace!(
                    self.logger,
                    "Block feedback because duplicate (reason: {:?}): {:?}",
                    feedback_reason,
                    source_feedback_value
                );
                return;
            }
        }
        trace!(
            self.logger,
            "Schedule sending feedback because {:?}: {:?}",
            feedback_reason,
            source_feedback_value
        );
        if let Some(test_sender) = self.channels.integration_test_feedback_sender.as_ref() {
            // Integration test
            // Test receiver could already be gone (if the test didn't wait long enough).
            let _ = test_sender.send(source_feedback_value);
        } else {
            // Production
            match (source_feedback_value, feedback_output) {
                (SourceFeedbackValue::Midi(v), FeedbackOutput::Midi(midi_output)) => {
                    match midi_output {
                        MidiDestination::FxOutput => {
                            if self.output_logging_enabled {
                                log_feedback_output(
                                    &self.instance_id,
                                    format_midi_source_value(&v),
                                );
                            }
                            self.channels
                                .feedback_real_time_task_sender
                                .send(FeedbackRealTimeTask::FxOutputFeedback(v))
                                .unwrap();
                        }
                        MidiDestination::Device(dev_id) => {
                            // We send to the audio hook in this case (the default case) because there's
                            // only one audio hook (not one per instance as with real-time processors),
                            // so it can guarantee us a globally deterministic order. This is necessary
                            // to achieve "eventual feedback consistency" by using instance
                            // orchestration techniques in the main thread. If
                            // we don't do that, we can prepare the most perfect
                            // feedback ordering in the backbone control surface (main
                            // thread, in order to support multiple instances with the same device) ...
                            // it won't be useful at all if the real-time processors send the feedback
                            // in the order of instance instantiation.
                            if self.output_logging_enabled {
                                log_feedback_output(
                                    &self.instance_id,
                                    format_midi_source_value(&v),
                                );
                            }
                            self.channels
                                .feedback_audio_hook_task_sender
                                .send(FeedbackAudioHookTask::MidiDeviceFeedback(dev_id, v))
                                .unwrap();
                        }
                    }
                }
                (SourceFeedbackValue::Osc(msg), FeedbackOutput::Osc(dev_id)) => {
                    if self.output_logging_enabled {
                        log_feedback_output(&self.instance_id, format_osc_message(&msg));
                    }
                    self.channels
                        .osc_feedback_task_sender
                        .try_send(OscFeedbackTask::new(dev_id, msg))
                        .unwrap();
                }
                _ => {}
            }
        }
    }

    fn send_direct_feedback(
        &self,
        feedback_reason: FeedbackReason,
        feedback_value: RealFeedbackValue,
        is_feedback_after_control: bool,
    ) {
        if feedback_reason.is_always_allowed() || self.instance_feedback_is_effectively_enabled() {
            if let Some(feedback_output) = self.feedback_output {
                if let Some(source_feedback_value) = feedback_value.source {
                    // At this point we can be sure that this mapping can't have a
                    // virtual source.
                    if feedback_reason.is_source_release() {
                        // Possible interference with other instances. Don't switch off yet!
                        // Give other instances the chance to take over.
                        let event =
                            InstanceOrchestrationEvent::SourceReleased(SourceReleasedEvent {
                                instance_id: self.instance_id.to_owned(),
                                feedback_output,
                                feedback_value: source_feedback_value,
                            });
                        self.channels
                            .instance_orchestration_event_sender
                            .try_send(event)
                            .unwrap();
                    } else {
                        // Send feedback right now.
                        self.send_direct_source_feedback(
                            feedback_output,
                            feedback_reason,
                            source_feedback_value,
                            is_feedback_after_control,
                        );
                    }
                }
            }
        }
        if let Some(projection_feedback_value) = feedback_value.projection {
            self.event_handler
                .handle_event(DomainEvent::ProjectionFeedback(projection_feedback_value));
        }
    }

    pub fn instance_feedback_is_effectively_enabled(&self) -> bool {
        if let Some(fo) = self.feedback_output {
            self.feedback_is_globally_enabled
                && BackboneState::get().feedback_is_allowed(&self.instance_id, fo)
        } else {
            // Pointless but allowed
            true
        }
    }

    /// Processes main mappings with virtual sources.
    fn process_main_mappings_with_virtual_sources(
        &self,
        main_mappings: &mut OrderedMappingMap<MainMapping>,
        value: VirtualSourceValue,
        options: ControlOptions,
        parameters: &ParameterArray,
    ) -> Vec<ExtendedMappingControlResult> {
        // Controller mappings can't have virtual sources, so for now we only need to check
        // main mappings.
        let mut enforce_target_refresh = false;
        main_mappings
            .values_mut()
            .filter(|m| m.control_is_effectively_on())
            .filter_map(|m| {
                if let CompoundMappingSource::Virtual(s) = &m.source() {
                    let control_value = s.control(&value)?;
                    let options = ControlOptions {
                        enforce_target_refresh,
                        ..options
                    };
                    let control_result = control_mapping_stage_one_and_two(
                        self,
                        parameters,
                        m,
                        control_value,
                        options,
                        ManualFeedbackProcessing::Off,
                    );
                    if let Some(new_value) = control_result.new_target_value {
                        self.notify_target_value_changed(m, new_value);
                    }
                    enforce_target_refresh = true;
                    let extended_control_result = ExtendedMappingControlResult {
                        control_result,
                        compartment: m.compartment(),
                        group_interaction_input: GroupInteractionInput {
                            mapping_id: m.id(),
                            group_interaction: m.group_interaction(),
                            control_value,
                        },
                    };
                    Some(extended_control_result)
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Includes virtual mappings if the controller mapping compartment is queried.
fn all_mappings_in_compartment_mut<'a>(
    mappings: &'a mut EnumMap<MappingCompartment, OrderedMappingMap<MainMapping>>,
    mappings_with_virtual_targets: &'a mut OrderedMappingMap<MainMapping>,
    compartment: MappingCompartment,
) -> impl Iterator<Item = &'a mut MainMapping> {
    mappings[compartment].values_mut().chain(
        mappings_with_virtual_targets
            .values_mut()
            // Include virtual target mappings if we are talking about controller compartment.
            .filter(move |_| compartment == MappingCompartment::ControllerMappings),
    )
}

fn get_normal_or_virtual_target_mapping_mut<'a>(
    mappings: &'a mut EnumMap<MappingCompartment, OrderedMappingMap<MainMapping>>,
    mappings_with_virtual_targets: &'a mut OrderedMappingMap<MainMapping>,
    compartment: MappingCompartment,
    id: MappingId,
) -> Option<&'a mut MainMapping> {
    mappings[compartment].get_mut(&id).or(
        if compartment == MappingCompartment::ControllerMappings {
            mappings_with_virtual_targets.get_mut(&id)
        } else {
            None
        },
    )
}

// At the moment based on a SmallAsciiString. When changing this in future, e.g. to UUID, take care
// of implementing Display in a way that outputs something like nanoid! because this will be used
// as the initial session ID - which should be a bit more human-friendly than UUIDs.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct InstanceId(SmallAsciiString);

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl InstanceId {
    pub fn random() -> Self {
        let instance_id = nanoid::nanoid!(8);
        let ascii_string: AsciiString = instance_id
            .chars()
            .filter_map(|c| c.to_ascii_char().ok())
            .collect();
        Self(SmallAsciiString::from_ascii_str_cropping(&ascii_string))
    }
}

#[derive(Display)]
pub enum InputMatchResult {
    #[display(fmt = "consumed")]
    Consumed,
    #[display(fmt = "unmatched")]
    Unmatched,
    #[display(fmt = "matched")]
    Matched,
}

#[must_use]
fn control_mapping_stage_one_and_two<EH: DomainEventHandler>(
    basics: &Basics<EH>,
    parameters: &ParameterArray,
    m: &mut MainMapping,
    control_value: ControlValue,
    options: ControlOptions,
    feedback_handling: ManualFeedbackProcessing,
) -> MappingControlResult {
    let mut control_result =
        control_mapping_stage_one(basics, parameters, m, control_value, options);
    control_mapping_stage_two(basics, &mut control_result, m, feedback_handling);
    control_result
}

/// Executes stage one of a typical mapping control invocation.
///
/// Takes care of:
///
/// 1. Notifying that mapping matched
/// 2. Controlling with given control value (probably produced by source) starting from mode.
#[must_use]
fn control_mapping_stage_one<EH: DomainEventHandler>(
    basics: &Basics<EH>,
    parameters: &ParameterArray,
    m: &mut MainMapping,
    control_value: ControlValue,
    options: ControlOptions,
) -> MappingControlResult {
    basics
        .event_handler
        .notify_mapping_matched(m.compartment(), m.id());
    m.control_from_mode(
        control_value,
        options,
        basics.control_context(),
        &basics.logger,
        ExtendedProcessorContext::new(&basics.context, parameters, basics.control_context()),
    )
}

/// Executes stage one of a typical mapping control invocation.
///
/// Takes care of:
///
/// 1. Sending manual feedback due to target or "Send feedback after control".
fn control_mapping_stage_two<EH: DomainEventHandler>(
    basics: &Basics<EH>,
    control_result: &mut MappingControlResult,
    m: &mut MainMapping,
    feedback_handling: ManualFeedbackProcessing,
) {
    if let Some(new_value) = control_result.new_target_value {
        basics.notify_target_value_changed(m, new_value);
    }
    if let ManualFeedbackProcessing::On {
        mappings_with_virtual_targets,
    } = feedback_handling
    {
        basics.send_feedback(
            mappings_with_virtual_targets,
            FeedbackReason::Normal,
            control_result.feedback_value.take(),
        );
    }
}

/// Executes stage three of a typical mapping control invocation.
///
/// Takes care of:
///
/// 1. Executing a possible hit instruction (and in a possible second pass all resulting hit
///    instructions). A second pass is not just theory, it makes a lot of sense in practice, e.g.
///    when we control "Enable/disable mappings" via "Navigate within group". However, we should
///    stop there in order to prevent infinite loops. If we really need more in future, we can add
///    a third pass.  
/// 2. Processing group interaction (if enabled).
fn control_mapping_stage_three<EH: DomainEventHandler>(
    basics: &Basics<EH>,
    collections: &mut Collections,
    compartment: MappingCompartment,
    control_result: MappingControlResult,
    group_interaction_processing: GroupInteractionProcessing,
) {
    if let Some(hi) = control_result.hit_instruction {
        let control_context = basics.control_context();
        let processor_context = ExtendedProcessorContext::new(
            &basics.context,
            &collections.parameters,
            control_context,
        );
        let pass_2_control_results = hi.execute(HitInstructionContext {
            mappings: &mut collections.mappings[compartment],
            control_context,
            domain_event_handler: &basics.event_handler,
            logger: &basics.logger,
            processor_context,
        });
        // Second pass, without group interaction this time!
        for pass_2_control_result in pass_2_control_results {
            if let Some(pass_2_hi) = pass_2_control_result.hit_instruction {
                pass_2_hi.execute(HitInstructionContext {
                    mappings: &mut collections.mappings[compartment],
                    control_context,
                    domain_event_handler: &basics.event_handler,
                    logger: &basics.logger,
                    processor_context,
                });
            }
        }
    }
    if let GroupInteractionProcessing::On(input) = group_interaction_processing {
        if input.group_interaction != GroupInteraction::None {
            basics.process_group_interaction(
                collections,
                compartment,
                input.mapping_id,
                input.control_value,
                control_result.successful,
            );
        }
    }
}

enum ManualFeedbackProcessing<'a> {
    Off,
    On {
        mappings_with_virtual_targets: &'a OrderedMappingMap<MainMapping>,
    },
}

enum GroupInteractionProcessing {
    Off,
    On(GroupInteractionInput),
}

struct ExtendedMappingControlResult {
    control_result: MappingControlResult,
    compartment: MappingCompartment,
    group_interaction_input: GroupInteractionInput,
}

struct GroupInteractionInput {
    mapping_id: MappingId,
    group_interaction: GroupInteraction,
    control_value: ControlValue,
}

struct Fb(FeedbackReason, Option<CompoundFeedbackValue>);
impl Fb {
    fn none() -> Self {
        Fb(FeedbackReason::Normal, None)
    }

    fn unused(value: Option<CompoundFeedbackValue>) -> Self {
        Fb(FeedbackReason::ClearUnusedSource, value)
    }

    fn normal(value: Option<CompoundFeedbackValue>) -> Self {
        Fb(FeedbackReason::Normal, value)
    }
}
