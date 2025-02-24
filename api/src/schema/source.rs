pub use midi::*;
pub use osc::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use virt::*;

#[derive(PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
#[allow(clippy::enum_variant_names)]
pub enum Source {
    // None
    #[serde(rename = "None")]
    NoneSource,
    // REAPER
    MidiDeviceChanges(MidiDeviceChangesSource),
    RealearnInstanceStart(RealearnInstanceStartSource),
    // MIDI
    MidiNoteVelocity(MidiNoteVelocitySource),
    MidiNoteKeyNumber(MidiNoteKeyNumberSource),
    MidiPolyphonicKeyPressureAmount(MidiPolyphonicKeyPressureAmountSource),
    MidiControlChangeValue(MidiControlChangeValueSource),
    MidiProgramChangeNumber(MidiProgramChangeNumberSource),
    MidiChannelPressureAmount(MidiChannelPressureAmountSource),
    MidiPitchBendChangeValue(MidiPitchBendChangeValueSource),
    MidiParameterNumberValue(MidiParameterNumberValueSource),
    MidiClockTempo(MidiClockTempoSource),
    MidiClockTransport(MidiClockTransportSource),
    MidiRaw(MidiRawSource),
    MidiScript(MidiScriptSource),
    MackieLcd(MackieLcdSource),
    MackieSevenSegmentDisplay(MackieSevenSegmentDisplaySource),
    SiniConE24Display(SiniConE24DisplaySource),
    LaunchpadProScrollingTextDisplay(LaunchpadProScrollingTextDisplaySource),
    // OSC
    Osc(OscSource),
    Virtual(VirtualSource),
}

impl Default for Source {
    fn default() -> Self {
        Source::NoneSource
    }
}

// Only makes sense for sources that support both control *and* feedback.
#[derive(Copy, Clone, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum FeedbackBehavior {
    Normal,
    SendFeedbackAfterControl,
    PreventEchoFeedback,
}

impl Default for FeedbackBehavior {
    fn default() -> Self {
        Self::Normal
    }
}

mod midi {
    use crate::schema::FeedbackBehavior;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiNoteVelocitySource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_behavior: Option<FeedbackBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub channel: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub key_number: Option<u8>,
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiNoteKeyNumberSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_behavior: Option<FeedbackBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub channel: Option<u8>,
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiPolyphonicKeyPressureAmountSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_behavior: Option<FeedbackBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub channel: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub key_number: Option<u8>,
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiControlChangeValueSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_behavior: Option<FeedbackBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub channel: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub controller_number: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub character: Option<SourceCharacter>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub fourteen_bit: Option<bool>,
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiProgramChangeNumberSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_behavior: Option<FeedbackBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub channel: Option<u8>,
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiChannelPressureAmountSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_behavior: Option<FeedbackBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub channel: Option<u8>,
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiPitchBendChangeValueSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_behavior: Option<FeedbackBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub channel: Option<u8>,
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiParameterNumberValueSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_behavior: Option<FeedbackBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub channel: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub number: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub fourteen_bit: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub registered: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub character: Option<SourceCharacter>,
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiClockTempoSource;

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiDeviceChangesSource;

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct RealearnInstanceStartSource;

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiClockTransportSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub message: Option<MidiClockTransportMessage>,
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiRawSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_behavior: Option<FeedbackBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub pattern: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub character: Option<SourceCharacter>,
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MidiScriptSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub script: Option<String>,
    }

    #[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
    pub enum SourceCharacter {
        Range,
        Button,
        // 127 = decrement;  0 = none;  1 = increment
        Relative1,
        //  63 = decrement; 64 = none; 65 = increment
        Relative2,
        //  65 = decrement;  0 = none;  1 = increment
        Relative3,
        StatefulButton,
    }

    impl Default for SourceCharacter {
        fn default() -> Self {
            SourceCharacter::Range
        }
    }

    #[derive(Copy, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    pub enum MidiClockTransportMessage {
        Start,
        Continue,
        Stop,
    }

    impl Default for MidiClockTransportMessage {
        fn default() -> Self {
            MidiClockTransportMessage::Start
        }
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MackieLcdSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub channel: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub line: Option<u8>,
    }

    #[derive(Copy, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MackieSevenSegmentDisplaySource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub scope: Option<MackieSevenSegmentDisplayScope>,
    }

    #[derive(Copy, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    pub enum MackieSevenSegmentDisplayScope {
        All,
        Assignment,
        Tc,
        TcHoursBars,
        TcMinutesBeats,
        TcSecondsSub,
        TcFramesTicks,
    }

    impl Default for MackieSevenSegmentDisplayScope {
        fn default() -> Self {
            MackieSevenSegmentDisplayScope::Assignment
        }
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct SiniConE24DisplaySource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub cell_index: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub item_index: Option<u8>,
    }

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct LaunchpadProScrollingTextDisplaySource;
}

mod osc {
    use crate::schema::{FeedbackBehavior, OscArgument};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Default, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct OscSource {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub feedback_behavior: Option<FeedbackBehavior>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub address: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub argument: Option<OscArgument>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub relative: Option<bool>,
    }
}

mod virt {
    use crate::schema::{VirtualControlElementCharacter, VirtualControlElementId};
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct VirtualSource {
        pub id: VirtualControlElementId,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub character: Option<VirtualControlElementCharacter>,
    }
}
