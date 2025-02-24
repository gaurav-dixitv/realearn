use crate::base::eel;
use helgoboss_learn::Transformation;

use std::sync::Arc;

#[derive(Default)]
pub struct AdditionalEelTransformationInput {
    pub y_last: f64,
}

#[derive(Debug)]
struct EelUnit {
    // Declared above VM in order to be dropped before VM is dropped.
    program: eel::Program,
    vm: eel::Vm,
    x: eel::Variable,
    y: eel::Variable,
    y_last: eel::Variable,
}

#[derive(Clone, Debug)]
pub enum OutputVariable {
    X,
    Y,
}

/// Represents a value transformation done via EEL scripting language.
#[derive(Clone, Debug)]
pub struct EelTransformation {
    // Arc because EelUnit is not cloneable
    eel_unit: Arc<EelUnit>,
    output_var: OutputVariable,
}

impl EelTransformation {
    // Compiles the given script and creates an appropriate transformation.
    pub fn compile(
        eel_script: &str,
        result_var: OutputVariable,
    ) -> Result<EelTransformation, String> {
        if eel_script.trim().is_empty() {
            return Err("script empty".to_string());
        }
        let vm = eel::Vm::new();
        let program = vm.compile(eel_script)?;
        let x = vm.register_variable("x");
        let y = vm.register_variable("y");
        let y_last = vm.register_variable("y_last");
        let eel_unit = EelUnit {
            program,
            vm,
            x,
            y,
            y_last,
        };
        Ok(EelTransformation {
            eel_unit: Arc::new(eel_unit),
            output_var: result_var,
        })
    }
}

impl Transformation for EelTransformation {
    type AdditionalInput = AdditionalEelTransformationInput;

    fn transform(
        &self,
        input_value: f64,
        output_value: f64,
        additional_input: AdditionalEelTransformationInput,
    ) -> Result<f64, &'static str> {
        let result = unsafe {
            use OutputVariable::*;
            let (input_var, output_var) = match self.output_var {
                X => (&self.eel_unit.y, &self.eel_unit.x),
                Y => (&self.eel_unit.x, &self.eel_unit.y),
            };
            input_var.set(input_value);
            output_var.set(output_value);
            self.eel_unit.y_last.set(additional_input.y_last);
            self.eel_unit.program.execute();
            output_var.get()
        };
        Ok(result)
    }
}
