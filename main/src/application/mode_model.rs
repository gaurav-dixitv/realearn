use crate::base::{prop, Prop};
use crate::domain::{EelTransformation, Mode, OutputVariable};

use helgoboss_learn::{
    check_mode_applicability, full_discrete_interval, full_unit_interval, AbsoluteMode,
    ButtonUsage, DetailedSourceCharacter, DiscreteIncrement, EncoderUsage, FeedbackType, FireMode,
    GroupInteraction, Interval, ModeApplicabilityCheckInput, ModeParameter, ModeSettings,
    OutOfRangeBehavior, SoftSymmetricUnitValue, TakeoverMode, UnitValue, ValueSequence,
    VirtualColor,
};

use rxrust::prelude::*;

use std::time::Duration;

/// A model for creating modes
#[derive(Clone, Debug)]
pub struct ModeModel {
    pub r#type: Prop<AbsoluteMode>,
    pub target_value_interval: Prop<Interval<UnitValue>>,
    pub source_value_interval: Prop<Interval<UnitValue>>,
    pub reverse: Prop<bool>,
    pub press_duration_interval: Prop<Interval<Duration>>,
    pub turbo_rate: Prop<Duration>,
    pub jump_interval: Prop<Interval<UnitValue>>,
    pub out_of_range_behavior: Prop<OutOfRangeBehavior>,
    pub fire_mode: Prop<FireMode>,
    pub round_target_value: Prop<bool>,
    pub takeover_mode: Prop<TakeoverMode>,
    pub button_usage: Prop<ButtonUsage>,
    pub encoder_usage: Prop<EncoderUsage>,
    pub eel_control_transformation: Prop<String>,
    pub eel_feedback_transformation: Prop<String>,
    // For relative control values.
    /// Depending on the target character, this is either a step count or a step size.
    ///
    /// A step count is a coefficient which multiplies the atomic step size. E.g. a step count of 2
    /// can be read as 2 * step_size which means double speed. When the step count is negative,
    /// it's interpreted as a fraction of 1. E.g. a step count of -2 is 1/2 * step_size which
    /// means half speed. The increment is fired only every nth time, which results in a
    /// slow-down, or in other words, less sensitivity.
    ///
    /// A step size is the positive, absolute size of an increment. 0.0 represents no increment,
    /// 1.0 represents an increment over the whole value range (not very useful).
    ///
    /// It's an interval. When using rotary encoders, the most important value is the interval
    /// minimum. There are some controllers which deliver higher increments if turned faster. This
    /// is where the maximum comes in. The maximum is also important if using the relative mode
    /// with buttons. The harder you press the button, the higher the increment. It's limited
    /// by the maximum value.
    pub step_interval: Prop<Interval<SoftSymmetricUnitValue>>,
    pub rotate: Prop<bool>,
    pub make_absolute: Prop<bool>,
    pub group_interaction: Prop<GroupInteraction>,
    pub target_value_sequence: Prop<ValueSequence>,
    pub feedback_type: Prop<FeedbackType>,
    pub textual_feedback_expression: Prop<String>,
    pub feedback_color: Prop<Option<VirtualColor>>,
    pub feedback_background_color: Prop<Option<VirtualColor>>,
}

impl Default for ModeModel {
    fn default() -> Self {
        Self {
            r#type: prop(AbsoluteMode::Normal),
            target_value_interval: prop(full_unit_interval()),
            source_value_interval: prop(full_unit_interval()),
            reverse: prop(false),
            press_duration_interval: prop(Interval::new(
                Duration::from_millis(0),
                Duration::from_millis(0),
            )),
            turbo_rate: prop(Duration::from_millis(0)),
            jump_interval: prop(full_unit_interval()),
            out_of_range_behavior: prop(Default::default()),
            fire_mode: prop(Default::default()),
            round_target_value: prop(false),
            takeover_mode: prop(Default::default()),
            button_usage: prop(Default::default()),
            encoder_usage: prop(Default::default()),
            eel_control_transformation: prop(String::new()),
            eel_feedback_transformation: prop(String::new()),
            step_interval: prop(Self::default_step_size_interval()),
            rotate: prop(false),
            make_absolute: prop(false),
            group_interaction: prop(Default::default()),
            target_value_sequence: prop(Default::default()),
            feedback_type: prop(Default::default()),
            textual_feedback_expression: prop(Default::default()),
            feedback_color: prop(Default::default()),
            feedback_background_color: prop(Default::default()),
        }
    }
}

impl ModeModel {
    pub fn default_step_size_interval() -> Interval<SoftSymmetricUnitValue> {
        // 0.01 has been chosen as default minimum step size because it corresponds to 1%.
        //
        // 0.05 has been chosen as default maximum step size in order to make users aware that
        // ReaLearn supports encoder acceleration ("dial harder = more increments") and
        // velocity-sensitive buttons ("press harder = more increments") but still is low
        // enough to not lead to surprising results such as ugly parameter jumps.
        Interval::new(
            SoftSymmetricUnitValue::new(0.01),
            SoftSymmetricUnitValue::new(0.05),
        )
    }

    /// This doesn't reset the mode type, just all the values.
    pub fn reset_within_type(&mut self) {
        let def = ModeModel::default();
        self.source_value_interval
            .set(def.source_value_interval.get());
        self.target_value_interval
            .set(def.target_value_interval.get());
        self.jump_interval.set(def.jump_interval.get());
        self.eel_control_transformation
            .set(def.eel_control_transformation.get_ref().clone());
        self.eel_feedback_transformation
            .set(def.eel_feedback_transformation.get_ref().clone());
        self.textual_feedback_expression
            .set(def.textual_feedback_expression.get_ref().clone());
        self.feedback_color
            .set(def.feedback_color.get_ref().clone());
        self.feedback_background_color
            .set(def.feedback_background_color.get_ref().clone());
        self.out_of_range_behavior
            .set(def.out_of_range_behavior.get());
        self.fire_mode.set(def.fire_mode.get());
        self.round_target_value.set(def.round_target_value.get());
        self.takeover_mode.set(def.takeover_mode.get());
        self.button_usage.set(def.button_usage.get());
        self.encoder_usage.set(def.encoder_usage.get());
        self.rotate.set(def.rotate.get());
        self.make_absolute.set(def.make_absolute.get());
        self.group_interaction.set(def.group_interaction.get());
        self.target_value_sequence
            .set(def.target_value_sequence.get_ref().clone());
        self.feedback_type.set(def.feedback_type.get());
        self.reverse.set(def.reverse.get());
        self.step_interval.set(def.step_interval.get());
        self.press_duration_interval
            .set(def.press_duration_interval.get());
        self.turbo_rate.set(def.turbo_rate.get());
    }

    /// Fires whenever one of the properties of this model has changed
    pub fn changed(&self) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.r#type
            .changed()
            .merge(self.target_value_interval.changed())
            .merge(self.source_value_interval.changed())
            .merge(self.reverse.changed())
            .merge(self.jump_interval.changed())
            .merge(self.out_of_range_behavior.changed())
            .merge(self.fire_mode.changed())
            .merge(self.round_target_value.changed())
            .merge(self.takeover_mode.changed())
            .merge(self.button_usage.changed())
            .merge(self.encoder_usage.changed())
            .merge(self.eel_control_transformation.changed())
            .merge(self.eel_feedback_transformation.changed())
            .merge(self.textual_feedback_expression.changed())
            .merge(self.feedback_color.changed())
            .merge(self.feedback_background_color.changed())
            .merge(self.step_interval.changed())
            .merge(self.rotate.changed())
            .merge(self.press_duration_interval.changed())
            .merge(self.turbo_rate.changed())
            .merge(self.make_absolute.changed())
            .merge(self.group_interaction.changed())
            .merge(self.target_value_sequence.changed())
            .merge(self.feedback_type.changed())
    }

    pub fn mode_parameter_is_relevant(
        &self,
        mode_parameter: ModeParameter,
        base_input: ModeApplicabilityCheckInput,
        possible_source_characters: &[DetailedSourceCharacter],
        control_is_relevant: bool,
        feedback_is_relevant: bool,
    ) -> bool {
        possible_source_characters.iter().any(|source_character| {
            let is_applicable = |is_feedback| {
                let input = ModeApplicabilityCheckInput {
                    is_feedback,
                    mode_parameter,
                    source_character: *source_character,
                    ..base_input
                };
                check_mode_applicability(input).is_relevant()
            };
            (control_is_relevant && is_applicable(false))
                || (feedback_is_relevant && is_applicable(true))
        })
    }

    /// Creates a mode reflecting this model's current values
    #[allow(clippy::if_same_then_else)]
    pub fn create_mode(
        &self,
        base_input: ModeApplicabilityCheckInput,
        possible_source_characters: &[DetailedSourceCharacter],
    ) -> Mode {
        let is_relevant = |mode_parameter: ModeParameter| {
            // We take both control and feedback into account to not accidentally get slightly
            // different behavior if feedback is not enabled.
            self.mode_parameter_is_relevant(
                mode_parameter,
                base_input,
                possible_source_characters,
                true,
                true,
            )
        };
        // We know that just step max sometimes needs to be set to a sensible default (= step min)
        // and we know that step size and speed is mutually exclusive and therefore doesn't need
        // to be handled separately.
        let step_max_is_relevant =
            is_relevant(ModeParameter::StepSizeMax) || is_relevant(ModeParameter::SpeedMax);
        let min_step_count = convert_to_step_count(self.step_interval.get_ref().min_val());
        let min_step_size = self.step_interval.get_ref().min_val().abs();
        Mode::new(ModeSettings {
            absolute_mode: if is_relevant(ModeParameter::AbsoluteMode) {
                self.r#type.get()
            } else {
                AbsoluteMode::default()
            },
            source_value_interval: if is_relevant(ModeParameter::SourceMinMax) {
                self.source_value_interval.get()
            } else {
                full_unit_interval()
            },
            discrete_source_value_interval: if is_relevant(ModeParameter::SourceMinMax) {
                // TODO-high-discrete Use dedicated discrete source interval
                full_discrete_interval()
            } else {
                full_discrete_interval()
            },
            target_value_interval: if is_relevant(ModeParameter::TargetMinMax) {
                self.target_value_interval.get()
            } else {
                full_unit_interval()
            },
            discrete_target_value_interval: if is_relevant(ModeParameter::TargetMinMax) {
                // TODO-high-discrete Use dedicated discrete target interval
                full_discrete_interval()
            } else {
                full_discrete_interval()
            },
            step_count_interval: Interval::new(
                min_step_count,
                if step_max_is_relevant {
                    convert_to_step_count(self.step_interval.get_ref().max_val())
                } else {
                    min_step_count
                },
            ),
            step_size_interval: Interval::new_auto(
                min_step_size,
                if step_max_is_relevant {
                    self.step_interval.get_ref().max_val().abs()
                } else {
                    min_step_size
                },
            ),
            jump_interval: if is_relevant(ModeParameter::JumpMinMax) {
                self.jump_interval.get()
            } else {
                full_unit_interval()
            },
            discrete_jump_interval: if is_relevant(ModeParameter::JumpMinMax) {
                // TODO-high-discrete Use dedicated discrete jump interval
                full_discrete_interval()
            } else {
                full_discrete_interval()
            },
            fire_mode: if is_relevant(ModeParameter::FireMode) {
                self.fire_mode.get()
            } else {
                FireMode::default()
            },
            press_duration_interval: self.press_duration_interval.get(),
            turbo_rate: self.turbo_rate.get(),
            takeover_mode: if is_relevant(ModeParameter::TakeoverMode) {
                self.takeover_mode.get()
            } else {
                TakeoverMode::default()
            },
            encoder_usage: if is_relevant(ModeParameter::RelativeFilter) {
                self.encoder_usage.get()
            } else {
                EncoderUsage::default()
            },
            button_usage: if is_relevant(ModeParameter::ButtonFilter) {
                self.button_usage.get()
            } else {
                ButtonUsage::default()
            },
            reverse: if is_relevant(ModeParameter::Reverse) {
                self.reverse.get()
            } else {
                false
            },
            rotate: if is_relevant(ModeParameter::Rotate) {
                self.rotate.get()
            } else {
                false
            },
            round_target_value: if is_relevant(ModeParameter::RoundTargetValue) {
                self.round_target_value.get()
            } else {
                false
            },
            out_of_range_behavior: if is_relevant(ModeParameter::OutOfRangeBehavior) {
                self.out_of_range_behavior.get()
            } else {
                OutOfRangeBehavior::default()
            },
            control_transformation: if is_relevant(ModeParameter::ControlTransformation) {
                EelTransformation::compile(
                    self.eel_control_transformation.get_ref(),
                    OutputVariable::Y,
                )
                .ok()
            } else {
                None
            },
            feedback_transformation: if is_relevant(ModeParameter::FeedbackTransformation) {
                EelTransformation::compile(
                    self.eel_feedback_transformation.get_ref(),
                    OutputVariable::X,
                )
                .ok()
            } else {
                None
            },
            convert_relative_to_absolute: if is_relevant(ModeParameter::MakeAbsolute) {
                self.make_absolute.get()
            } else {
                false
            },
            // TODO-high-discrete Use discrete IF both source and target support it AND enabled
            use_discrete_processing: false,
            target_value_sequence: if is_relevant(ModeParameter::TargetValueSequence) {
                self.target_value_sequence.get_ref().clone()
            } else {
                Default::default()
            },
            feedback_type: self.feedback_type.get(),
            textual_feedback_expression: if is_relevant(ModeParameter::TextualFeedbackExpression) {
                self.textual_feedback_expression.get_ref().to_owned()
            } else {
                String::new()
            },
            feedback_color: self.feedback_color.get_ref().clone(),
            feedback_background_color: self.feedback_background_color.get_ref().clone(),
        })
    }
}

pub fn convert_factor_to_unit_value(factor: i32) -> SoftSymmetricUnitValue {
    let result = if factor == 0 {
        0.01
    } else {
        factor as f64 / 100.0
    };
    SoftSymmetricUnitValue::new(result)
}

pub fn convert_unit_value_to_factor(value: SoftSymmetricUnitValue) -> i32 {
    // -1.00 => -100
    // -0.01 =>   -1
    //  0.00 =>    1
    //  0.01 =>    1
    //  1.00 =>  100
    let tmp = (value.get() * 100.0).round() as i32;
    if tmp == 0 {
        1
    } else {
        tmp
    }
}

fn convert_to_step_count(value: SoftSymmetricUnitValue) -> DiscreteIncrement {
    DiscreteIncrement::new(convert_unit_value_to_factor(value))
}
