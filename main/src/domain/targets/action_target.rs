use crate::domain::ui_util::convert_bool_to_unit_value;
use crate::domain::{
    format_bool_as_on_off, get_effective_tracks, ActionInvocationType, AdditionalFeedbackEvent,
    CompoundChangeEvent, ControlContext, ExtendedProcessorContext, HitInstructionReturnValue,
    MappingCompartment, MappingControlContext, RealearnTarget, ReaperTarget, ReaperTargetType,
    TargetCharacter, TargetTypeDef, TrackDescriptor, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Fraction, Target, UnitValue};
use helgoboss_midi::U14;
use reaper_high::{Action, ActionCharacter, Project, Reaper, Track};
use reaper_medium::{ActionValueChange, CommandId, MasterTrackBehavior, WindowContext};
use std::convert::TryFrom;

#[derive(Debug)]
pub struct UnresolvedActionTarget {
    pub action: Action,
    pub invocation_type: ActionInvocationType,
    pub track_descriptor: Option<TrackDescriptor>,
}

impl UnresolvedReaperTargetDef for UnresolvedActionTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let project = context.context().project_or_current_project();
        let resolved_targets = if let Some(td) = &self.track_descriptor {
            get_effective_tracks(context, &td.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::Action(ActionTarget {
                        action: self.action.clone(),
                        invocation_type: self.invocation_type,
                        project,
                        track: Some(track),
                    })
                })
                .collect()
        } else {
            vec![ReaperTarget::Action(ActionTarget {
                action: self.action.clone(),
                invocation_type: self.invocation_type,
                project,
                track: None,
            })]
        };
        Ok(resolved_targets)
    }

    fn track_descriptor(&self) -> Option<&TrackDescriptor> {
        self.track_descriptor.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionTarget {
    pub action: Action,
    pub invocation_type: ActionInvocationType,
    pub project: Project,
    pub track: Option<Track>,
}

impl RealearnTarget for ActionTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        match self.invocation_type {
            ActionInvocationType::Trigger => (
                ControlType::AbsoluteContinuousRetriggerable,
                TargetCharacter::Switch,
            ),
            ActionInvocationType::Absolute => match self.action.character() {
                ActionCharacter::Toggle => {
                    (ControlType::AbsoluteContinuous, TargetCharacter::Switch)
                }
                ActionCharacter::Trigger => {
                    (ControlType::AbsoluteContinuous, TargetCharacter::Continuous)
                }
            },
            ActionInvocationType::Relative => (ControlType::Relative, TargetCharacter::Discrete),
        }
    }

    fn open(&self, _: ControlContext) {
        // Just open action window
        Reaper::get()
            .main_section()
            .action_by_command_id(CommandId::new(40605))
            .invoke_as_trigger(Some(self.project));
    }

    fn format_value(&self, _: UnitValue, _: ControlContext) -> String {
        "".to_owned()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if let Some(track) = &self.track {
            if !track.is_selected()
                || self
                    .project
                    .selected_track_count(MasterTrackBehavior::IncludeMasterTrack)
                    > 1
            {
                track.select_exclusively();
            }
        }
        match value {
            ControlValue::AbsoluteContinuous(v) => match self.invocation_type {
                ActionInvocationType::Trigger => {
                    if !v.is_zero() {
                        self.invoke_with_unit_value(v);
                    }
                }
                ActionInvocationType::Absolute => {
                    self.invoke_with_unit_value(v);
                }
                ActionInvocationType::Relative => {
                    return Err("relative invocation type can't take absolute values");
                }
            },
            ControlValue::Relative(i) => {
                if let ActionInvocationType::Relative = self.invocation_type {
                    self.action.invoke(i.get() as f64, true, Some(self.project));
                } else {
                    return Err("relative values need relative invocation type");
                }
            }
            ControlValue::AbsoluteDiscrete(f) => match self.invocation_type {
                ActionInvocationType::Trigger => {
                    if !f.is_zero() {
                        self.invoke_with_fraction(f)
                    }
                }
                ActionInvocationType::Absolute => self.invoke_with_fraction(f),
                ActionInvocationType::Relative => {
                    return Err("relative invocation type can't take absolute values");
                }
            },
        };
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.action.is_available()
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            // We can't provide a value from the event itself because the action hooks don't
            // pass values.
            CompoundChangeEvent::Additional(AdditionalFeedbackEvent::ActionInvoked(e))
                if e.command_id == self.action.command_id() =>
            {
                (true, None)
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, _: ControlContext) -> Option<String> {
        Some(format_bool_as_on_off(self.action.is_on()?).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::Action)
    }
}

impl<'a> Target<'a> for ActionTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = if let Some(state) = self.action.is_on() {
            // Toggle action: Return toggle state as 0 or 1.
            convert_bool_to_unit_value(state)
        } else {
            // Non-toggle action. Try to return current absolute value if this is a
            // MIDI CC/mousewheel action.
            if let Some(value) = self.action.normalized_value() {
                UnitValue::new(value)
            } else {
                UnitValue::MIN
            }
        };
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl ActionTarget {
    fn invoke_with_fraction(&self, f: Fraction) {
        if let Ok(u14) = U14::try_from(f.actual()) {
            self.action.invoke_directly(
                ActionValueChange::AbsoluteHighRes(u14),
                WindowContext::Win(Reaper::get().main_window()),
                self.project.context(),
            );
        }
    }

    fn invoke_with_unit_value(&self, v: UnitValue) {
        self.action.invoke(v.get(), false, Some(self.project))
    }
}

pub const ACTION_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Project: Invoke REAPER action",
    short_name: "Action",
    hint: "Limited feedback only",
    supports_track: true,
    if_so_supports_track_must_be_selected: false,
    ..DEFAULT_TARGET
};
