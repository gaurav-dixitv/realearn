use crate::domain::{
    classify_midi_message, Event, Garbage, GarbageBin, IncomingMidiMessage, InstanceId,
    MidiControlInput, MidiMessageClassification, MidiScanResult, MidiScanner, RealTimeProcessor,
};
use assert_no_alloc::*;
use helgoboss_learn::{MidiSourceValue, RawMidiEvent};
use helgoboss_midi::{DataEntryByteOrder, RawShortMessage};
use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};
use reaper_medium::{
    MidiEvent, MidiInputDeviceId, MidiOutputDeviceId, OnAudioBuffer, OnAudioBufferArgs,
    SendMidiTime,
};
use smallvec::SmallVec;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const AUDIO_HOOK_TASK_BULK_SIZE: usize = 1;
const FEEDBACK_TASK_BULK_SIZE: usize = 1000;

/// This needs to be thread-safe because if "Allow live FX multiprocessing" is active in the REAPER
/// preferences, the VST processing is executed in another thread than the audio hook!
pub type SharedRealTimeProcessor = Arc<Mutex<RealTimeProcessor>>;

pub type MidiCaptureSender = async_channel::Sender<MidiScanResult>;

// This kind of tasks is always processed, even after a rebirth when multiple processor syncs etc.
// have already accumulated. Because at the moment there's no way to request a full resync of all
// real-time processors from the control surface. In practice there's no danger that too many of
// those infrequent tasks accumulate so it's not an issue. Therefore the convention for now is to
// also send them when audio is not running.
pub enum NormalAudioHookTask {
    /// First parameter is the ID.
    //
    // Having the ID saves us from unnecessarily blocking the audio thread by looking into the
    // processor.
    AddRealTimeProcessor(InstanceId, SharedRealTimeProcessor),
    RemoveRealTimeProcessor(InstanceId),
    StartCapturingMidi(MidiCaptureSender),
    StopCapturingMidi,
}

/// A global feedback task (which is potentially sent very frequently).
#[derive(Debug)]
pub enum FeedbackAudioHookTask {
    MidiDeviceFeedback(
        MidiOutputDeviceId,
        MidiSourceValue<'static, RawShortMessage>,
    ),
    SendMidi(MidiOutputDeviceId, Vec<RawMidiEvent>),
}

#[derive(Debug)]
pub struct RealearnAudioHook {
    state: AudioHookState,
    real_time_processors: SmallVec<[(InstanceId, SharedRealTimeProcessor); 256]>,
    normal_task_receiver: crossbeam_channel::Receiver<NormalAudioHookTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackAudioHookTask>,
    time_of_last_run: Option<Instant>,
    garbage_bin: GarbageBin,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum AudioHookState {
    Normal,
    // This is not the instance-specific learning but the global one.
    LearningSource {
        sender: MidiCaptureSender,
        midi_scanner: MidiScanner,
    },
}

impl RealearnAudioHook {
    pub fn new(
        normal_task_receiver: crossbeam_channel::Receiver<NormalAudioHookTask>,
        feedback_task_receiver: crossbeam_channel::Receiver<FeedbackAudioHookTask>,
        garbage_bin: GarbageBin,
    ) -> RealearnAudioHook {
        Self {
            state: AudioHookState::Normal,
            real_time_processors: Default::default(),
            normal_task_receiver,
            feedback_task_receiver,
            time_of_last_run: None,
            garbage_bin,
        }
    }

    fn process_feedback_tasks(&mut self) {
        // Process global direct device feedback (since v2.8.0-pre6) - in order to
        // have deterministic feedback ordering, which is important for multi-instance
        // orchestration.
        for task in self
            .feedback_task_receiver
            .try_iter()
            .take(FEEDBACK_TASK_BULK_SIZE)
        {
            use FeedbackAudioHookTask::*;
            match task {
                MidiDeviceFeedback(dev_id, value) => {
                    if let Some(events) = value.to_raw() {
                        MidiOutputDevice::new(dev_id).with_midi_output(|mo| {
                            if let Some(mo) = mo {
                                for event in events {
                                    mo.send_msg(&*event, SendMidiTime::Instantly);
                                }
                            }
                        });
                    } else {
                        let shorts = value.to_short_messages(DataEntryByteOrder::MsbFirst);
                        if shorts[0].is_none() {
                            return;
                        }
                        MidiOutputDevice::new(dev_id).with_midi_output(|mo| {
                            if let Some(mo) = mo {
                                for short in shorts.iter().flatten() {
                                    mo.send(*short, SendMidiTime::Instantly);
                                }
                            }
                        });
                    }
                    if let Some(garbage) = value.into_garbage() {
                        self.garbage_bin.dispose(Garbage::RawMidiEvents(garbage));
                    }
                }
                SendMidi(dev_id, raw_midi_events) => {
                    MidiOutputDevice::new(dev_id).with_midi_output(|mo| {
                        if let Some(mo) = mo {
                            for event in &raw_midi_events {
                                mo.send_msg(&*event, SendMidiTime::Instantly);
                            }
                        }
                    });
                    self.garbage_bin
                        .dispose(Garbage::RawMidiEvents(raw_midi_events));
                }
            }
        }
    }

    fn call_real_time_processors(&mut self, args: &OnAudioBufferArgs, might_be_rebirth: bool) {
        match &mut self.state {
            AudioHookState::Normal => {
                self.call_real_time_processors_in_normal_state(args, might_be_rebirth);
            }
            AudioHookState::LearningSource {
                sender,
                midi_scanner,
            } => {
                for (_, p) in self.real_time_processors.iter() {
                    p.lock_recover()
                        .run_from_audio_hook_essential(args.len as _, might_be_rebirth);
                }
                for dev in Reaper::get().midi_input_devices() {
                    dev.with_midi_input(|mi| {
                        if let Some(mi) = mi {
                            for e in mi.get_read_buf() {
                                if let Some(res) = scan_midi(dev.id(), e, midi_scanner) {
                                    let _ = sender.try_send(res);
                                }
                            }
                        }
                    });
                }
                if let Some(res) = midi_scanner.poll() {
                    // Source detected via polling. Return to normal mode.
                    let _ = sender.try_send(res);
                }
            }
        };
    }

    fn call_real_time_processors_in_normal_state(
        &mut self,
        args: &OnAudioBufferArgs,
        might_be_rebirth: bool,
    ) {
        // 1a. Drive real-time processors and determine used MIDI devices "on the go".
        //
        // Calling the real-time processor *before* processing its remove task has
        // the benefit that it can still do some final work (e.g. clearing
        // LEDs by sending zero feedback) before it's removed. That's also
        // one of the reasons why we remove the real-time processor async by
        // sending a message. It's okay if it's around for one cycle after a
        // plug-in instance has unloaded (only the case if not the last instance).
        //
        let mut midi_dev_id_is_used = [false; MidiInputDeviceId::MAX_DEVICE_COUNT as usize];
        let mut midi_devs_used_at_all = false;
        for (_, p) in self.real_time_processors.iter() {
            // Since 1.12.0, we "drive" each plug-in instance's real-time processor
            // primarily by the global audio hook. See https://github.com/helgoboss/realearn/issues/84 why this is
            // better. We also call it by the plug-in `process()` method though in order
            // to be able to send MIDI to <FX output> and to
            // stop doing so synchronously if the plug-in is
            // gone.
            let mut guard = p.lock_recover();
            guard.run_from_audio_hook_all(args.len as _, might_be_rebirth);
            if guard.control_is_globally_enabled() {
                if let MidiControlInput::Device(dev_id) = guard.midi_control_input() {
                    midi_dev_id_is_used[dev_id.get() as usize] = true;
                    midi_devs_used_at_all = true;
                }
            }
        }
        // 1b. Forward MIDI events from MIDI devices to ReaLearn instances and filter
        //     them globally if desired by the instance.
        if midi_devs_used_at_all {
            self.distribute_midi_events_to_processors(args, &midi_dev_id_is_used);
        }
    }

    fn distribute_midi_events_to_processors(
        &mut self,
        args: &OnAudioBufferArgs,
        midi_dev_id_is_used: &[bool; MidiInputDeviceId::MAX_DEVICE_COUNT as usize],
    ) {
        for dev_id in 0..MidiInputDeviceId::MAX_DEVICE_COUNT {
            if !midi_dev_id_is_used[dev_id as usize] {
                continue;
            }
            let dev_id = MidiInputDeviceId::new(dev_id);
            MidiInputDevice::new(dev_id).with_midi_input(|mi| {
                if let Some(mi) = mi {
                    let event_list = mi.get_read_buf();
                    let mut bpos = 0;
                    while let Some(res) = event_list.enum_items(bpos) {
                        // Current control mode is checked further down the callstack. No need to
                        // check it here.
                        let our_event = match Event::from_reaper(res.midi_event, args.srate) {
                            Err(_) => continue,
                            Ok(e) => e,
                        };
                        let mut filter_out_event = false;
                        for (_, p) in self.real_time_processors.iter() {
                            let mut guard = p.lock_recover();
                            if guard.control_is_globally_enabled()
                                && guard.midi_control_input() == MidiControlInput::Device(dev_id)
                                && guard.process_incoming_midi_from_audio_hook(our_event)
                            {
                                filter_out_event = true;
                            }
                        }
                        if filter_out_event {
                            event_list.delete_item(bpos);
                        } else {
                            bpos = res.next_bpos;
                        }
                    }
                }
            });
        }
    }

    fn process_add_remove_tasks(&mut self) {
        for task in self
            .normal_task_receiver
            .try_iter()
            .take(AUDIO_HOOK_TASK_BULK_SIZE)
        {
            use NormalAudioHookTask::*;
            match task {
                AddRealTimeProcessor(id, p) => {
                    self.real_time_processors.push((id, p));
                }
                RemoveRealTimeProcessor(id) => {
                    if let Some(pos) = self.real_time_processors.iter().position(|(i, _)| i == &id)
                    {
                        let (_, proc) = self.real_time_processors.swap_remove(pos);
                        self.garbage_bin.dispose(Garbage::RealTimeProcessor(proc));
                    }
                }
                StartCapturingMidi(sender) => {
                    self.state = AudioHookState::LearningSource {
                        sender,
                        midi_scanner: Default::default(),
                    }
                }
                StopCapturingMidi => {
                    let last_state = std::mem::replace(&mut self.state, AudioHookState::Normal);
                    if let AudioHookState::LearningSource { sender, .. } = last_state {
                        self.garbage_bin.dispose(Garbage::MidiCaptureSender(sender));
                    }
                }
            }
        }
    }
}

impl OnAudioBuffer for RealearnAudioHook {
    fn call(&mut self, args: OnAudioBufferArgs) {
        assert_no_alloc(|| {
            if args.is_post {
                return;
            }
            let current_time = Instant::now();
            let time_of_last_run = self.time_of_last_run.replace(current_time);
            let might_be_rebirth = if let Some(time) = time_of_last_run {
                current_time.duration_since(time) > Duration::from_secs(1)
            } else {
                false
            };
            self.process_feedback_tasks();
            self.call_real_time_processors(&args, might_be_rebirth);
            self.process_add_remove_tasks();
        });
    }
}

fn scan_midi(
    dev_id: MidiInputDeviceId,
    evt: &MidiEvent,
    midi_scanner: &mut MidiScanner,
) -> Option<MidiScanResult> {
    let msg = IncomingMidiMessage::from_reaper(evt.message()).ok()?;
    if classify_midi_message(msg) != MidiMessageClassification::Normal {
        return None;
    }
    use IncomingMidiMessage::*;
    match msg {
        Short(short_msg) => midi_scanner.feed_short(short_msg, Some(dev_id)),
        SysEx(bytes) => {
            // It's okay here to temporarily permit allocation because crackling during learning
            // is not a showstopper.
            permit_alloc(|| MidiScanResult::try_from_bytes(bytes, Some(dev_id)).ok())
        }
    }
}

pub trait RealTimeProcessorLocker {
    fn lock_recover(&self) -> MutexGuard<RealTimeProcessor>;
}

impl RealTimeProcessorLocker for SharedRealTimeProcessor {
    /// This ignores poisoning, which is okay in our case because if the real-time
    /// processor has panicked, we will see it in the REAPER console. No need to
    /// hide that error with lots of follow-up poisoning errors! This is a kind of
    /// recovery mechanism.
    fn lock_recover(&self) -> MutexGuard<RealTimeProcessor> {
        match self.lock() {
            Ok(guard) => guard,
            Err(e) => e.into_inner(),
        }
    }
}
