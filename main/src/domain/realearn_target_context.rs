use crate::domain::{
    AdditionalFeedbackEvent, FxSnapshotLoadedEvent, ParameterAutomationTouchStateChangedEvent,
    TouchedParameterType,
};
use reaper_high::{Fx, Track};
use reaper_medium::MediaTrack;
use std::collections::{HashMap, HashSet};

/// Feedback for most targets comes from REAPER itself but there are some targets for which ReaLearn
/// holds the state. It's in this struct.
pub struct RealearnTargetContext {
    additional_feedback_event_sender: crossbeam_channel::Sender<AdditionalFeedbackEvent>,
    // For "Load FX snapshot" target.
    fx_snapshot_chunk_hash_by_fx: HashMap<Fx, u64>,
    // For "Touch automation state" target.
    touched_things: HashSet<TouchedThing>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
struct TouchedThing {
    track: MediaTrack,
    parameter_type: TouchedParameterType,
}

impl TouchedThing {
    pub fn new(track: MediaTrack, parameter_type: TouchedParameterType) -> Self {
        Self {
            track,
            parameter_type,
        }
    }
}

impl RealearnTargetContext {
    pub fn new(
        additional_feedback_event_sender: crossbeam_channel::Sender<AdditionalFeedbackEvent>,
    ) -> Self {
        Self {
            fx_snapshot_chunk_hash_by_fx: Default::default(),
            additional_feedback_event_sender,
            touched_things: Default::default(),
        }
    }

    pub fn current_fx_snapshot_chunk_hash(&self, fx: &Fx) -> Option<u64> {
        self.fx_snapshot_chunk_hash_by_fx.get(fx).copied()
    }

    pub fn load_fx_snapshot(
        &mut self,
        fx: Fx,
        chunk: &str,
        chunk_hash: u64,
    ) -> Result<(), &'static str> {
        fx.set_tag_chunk(chunk)?;
        self.fx_snapshot_chunk_hash_by_fx
            .insert(fx.clone(), chunk_hash);
        self.additional_feedback_event_sender
            .try_send(AdditionalFeedbackEvent::FxSnapshotLoaded(
                FxSnapshotLoadedEvent { fx },
            ))
            .unwrap();
        Ok(())
    }

    pub fn touch_automation_parameter(
        &mut self,
        track: &Track,
        parameter_type: TouchedParameterType,
    ) {
        self.touched_things
            .insert(TouchedThing::new(track.raw(), parameter_type));
        self.post_process_touch(track, parameter_type);
        self.additional_feedback_event_sender
            .try_send(
                AdditionalFeedbackEvent::ParameterAutomationTouchStateChanged(
                    ParameterAutomationTouchStateChangedEvent {
                        track: track.raw(),
                        parameter_type,
                        new_value: true,
                    },
                ),
            )
            .unwrap();
    }

    pub fn untouch_automation_parameter(
        &mut self,
        track: &Track,
        parameter_type: TouchedParameterType,
    ) {
        self.touched_things
            .remove(&TouchedThing::new(track.raw(), parameter_type));
        self.additional_feedback_event_sender
            .try_send(
                AdditionalFeedbackEvent::ParameterAutomationTouchStateChanged(
                    ParameterAutomationTouchStateChangedEvent {
                        track: track.raw(),
                        parameter_type,
                        new_value: false,
                    },
                ),
            )
            .unwrap();
    }

    fn post_process_touch(&mut self, track: &Track, parameter_type: TouchedParameterType) {
        match parameter_type {
            TouchedParameterType::Volume => {
                track.set_volume(track.volume());
            }
            TouchedParameterType::Pan => {
                track.set_pan(track.pan());
            }
            TouchedParameterType::Width => {
                track.set_width(track.width());
            }
        }
    }

    pub fn automation_parameter_is_touched(
        &self,
        track: MediaTrack,
        parameter_type: TouchedParameterType,
    ) -> bool {
        self.touched_things
            .contains(&TouchedThing::new(track, parameter_type))
    }
}
