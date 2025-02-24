use crate::domain::{
    get_track_color, get_track_name, CompoundChangeEvent, CompoundMappingTarget, ControlContext,
    FeedbackResolution, MainMapping, RealearnTarget, ReaperTarget, UnresolvedCompoundMappingTarget,
};
use enum_dispatch::enum_dispatch;
use helgoboss_learn::{PropValue, Target};
use reaper_high::ChangeEvent;
use std::str::FromStr;
use strum_macros::EnumString;

/// `None` means that no polling is necessary for feedback because we are notified via events.
pub fn prop_feedback_resolution(
    key: &str,
    mapping: &MainMapping,
    target: &UnresolvedCompoundMappingTarget,
) -> Option<FeedbackResolution> {
    match key.parse::<Props>().ok() {
        Some(props) => props.feedback_resolution(mapping, target),
        None => {
            // Maybe target-specific placeholder. At the moment we should only have target-specific
            // placeholders whose feedback resolution is the same resolution as the one of the
            // main target value, so the following is good enough. If this changes in future, we
            // should introduce a similar function in ReaLearn target (one that takes a key).
            target.feedback_resolution()
        }
    }
}

pub fn prop_is_affected_by(
    key: &str,
    event: CompoundChangeEvent,
    mapping: &MainMapping,
    target: &ReaperTarget,
    control_context: ControlContext,
) -> bool {
    match key.parse::<Props>().ok() {
        Some(props) => {
            // TODO-medium Not very consequent? Here we take the first target and for
            //  target-specific placeholders the given one. A bit hard to change though. Let's see.
            props.is_affected_by(event, mapping, mapping.targets().first(), control_context)
        }
        None => {
            // Maybe target-specific placeholder. At the moment we should only have target-specific
            // placeholders that are affected by changes of the main target value, so the following
            // is good enough. If this changes in future, we should introduce a similar function
            // in ReaLearn target (one that takes a key).
            if key.starts_with("target.") {
                target.process_change_event(event, control_context).0
            } else {
                false
            }
        }
    }
}

pub fn get_prop_value(
    key: &str,
    mapping: &MainMapping,
    control_context: ControlContext,
) -> Option<PropValue> {
    match key.parse::<Props>().ok() {
        Some(props) => props.get_value(mapping, mapping.targets().first(), control_context),
        None => {
            if let (Some(key), Some(target)) =
                (key.strip_prefix("target."), mapping.targets().first())
            {
                target.prop_value(key, control_context)
            } else {
                None
            }
        }
    }
}

enum Props {
    Mapping(MappingProps),
    Target(TargetProps),
}

impl Props {
    /// `None` means that no polling is necessary for feedback because we are notified via events.
    pub fn feedback_resolution(
        &self,
        mapping: &MainMapping,
        target: &UnresolvedCompoundMappingTarget,
    ) -> Option<FeedbackResolution> {
        match self {
            Props::Mapping(p) => {
                let args = PropFeedbackResolutionArgs { object: mapping };
                p.feedback_resolution(args)
            }
            Props::Target(p) => {
                let args = PropFeedbackResolutionArgs {
                    object: MappingAndUnresolvedTarget { mapping, target },
                };
                p.feedback_resolution(args)
            }
        }
    }

    /// Returns whether the value of this property could be affected by the given change event.
    pub fn is_affected_by(
        &self,
        event: CompoundChangeEvent,
        mapping: &MainMapping,
        target: Option<&CompoundMappingTarget>,
        control_context: ControlContext,
    ) -> bool {
        match self {
            Props::Mapping(p) => {
                let args = PropIsAffectedByArgs {
                    event,
                    object: mapping,
                    control_context,
                };
                p.is_affected_by(args)
            }
            Props::Target(p) => target
                .map(|target| {
                    let args = PropIsAffectedByArgs {
                        event,
                        object: MappingAndTarget { mapping, target },
                        control_context,
                    };
                    p.is_affected_by(args)
                })
                .unwrap_or(false),
        }
    }

    /// Returns the current value of this property.
    pub fn get_value(
        &self,
        mapping: &MainMapping,
        target: Option<&CompoundMappingTarget>,
        control_context: ControlContext,
    ) -> Option<PropValue> {
        match self {
            Props::Mapping(p) => {
                let args = PropGetValueArgs {
                    object: mapping,
                    control_context,
                };
                p.get_value(args)
            }
            Props::Target(p) => target.and_then(|target| {
                let args = PropGetValueArgs {
                    object: MappingAndTarget { mapping, target },
                    control_context,
                };
                p.get_value(args)
            }),
        }
    }
}

impl FromStr for Props {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<MappingProps>()
            .map(Props::Mapping)
            .or_else(|_| s.parse::<TargetProps>().map(Props::Target))
    }
}

#[enum_dispatch]
#[derive(EnumString)]
enum MappingProps {
    #[strum(serialize = "mapping.name")]
    Name(MappingNameProp),
}

#[enum_dispatch]
#[derive(EnumString)]
enum TargetProps {
    #[strum(serialize = "target.type.name")]
    TargetTypeName(TargetTypeNameProp),
    #[strum(serialize = "target.type.long_name")]
    TargetTypeLongName(TargetTypeLongNameProp),
    #[strum(serialize = "target.text_value")]
    TextValue(TargetTextValueProp),
    #[strum(serialize = "target.numeric_value")]
    NumericValue(TargetNumericValueProp),
    #[strum(serialize = "target.numeric_value.unit")]
    NumericValueUnit(TargetNumericValueUnitProp),
    #[strum(serialize = "target.normalized_value")]
    NormalizedValue(TargetNormalizedValueProp),
    #[strum(serialize = "target.track.index")]
    TrackIndex(TargetTrackIndexProp),
    #[strum(serialize = "target.track.name")]
    TrackName(TargetTrackNameProp),
    #[strum(serialize = "target.track.color")]
    TrackColor(TargetTrackColorProp),
    #[strum(serialize = "target.fx.index")]
    FxIndex(TargetFxIndexProp),
    #[strum(serialize = "target.fx.name")]
    FxName(TargetFxNameProp),
    #[strum(serialize = "target.route.index")]
    RouteIndex(TargetRouteIndexProp),
    #[strum(serialize = "target.route.name")]
    RouteName(TargetRouteNameProp),
}

#[enum_dispatch(MappingProps)]
trait MappingProp {
    /// `None` means that no polling is necessary for feedback because we are notified via events.
    fn feedback_resolution(
        &self,
        args: PropFeedbackResolutionArgs<&MainMapping>,
    ) -> Option<FeedbackResolution> {
        let _ = args;
        None
    }

    /// Returns whether the value of this property could be affected by the given change event.
    fn is_affected_by(&self, args: PropIsAffectedByArgs<&MainMapping>) -> bool;

    /// Returns the current value of this property.
    fn get_value(&self, args: PropGetValueArgs<&MainMapping>) -> Option<PropValue>;
}

#[enum_dispatch(TargetProps)]
trait TargetProp {
    /// `None` means that no polling is necessary for feedback because we are notified via events.
    fn feedback_resolution(
        &self,
        args: PropFeedbackResolutionArgs<MappingAndUnresolvedTarget>,
    ) -> Option<FeedbackResolution> {
        let _ = args;
        None
    }

    /// Returns whether the value of this property could be affected by the given change event.
    fn is_affected_by(&self, args: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // Many target props change whenever the main target value changes. So this is the default.
        args.object
            .target
            .process_change_event(args.event, args.control_context)
            .0
    }

    /// Returns the current value of this property.
    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue>;
}

#[allow(dead_code)]
struct MappingAndTarget<'a> {
    pub mapping: &'a MainMapping,
    pub target: &'a CompoundMappingTarget,
}

#[allow(dead_code)]
struct MappingAndUnresolvedTarget<'a> {
    pub mapping: &'a MainMapping,
    pub target: &'a UnresolvedCompoundMappingTarget,
}

#[allow(dead_code)]
struct PropFeedbackResolutionArgs<T> {
    object: T,
}

struct PropIsAffectedByArgs<'a, T> {
    event: CompoundChangeEvent<'a>,
    object: T,
    control_context: ControlContext<'a>,
}

struct PropGetValueArgs<'a, T> {
    object: T,
    control_context: ControlContext<'a>,
}

#[derive(Default)]
struct MappingNameProp;

impl MappingProp for MappingNameProp {
    fn is_affected_by(&self, _: PropIsAffectedByArgs<&MainMapping>) -> bool {
        // Mapping name changes will result in a full mapping resync anyway.
        false
    }

    fn get_value(&self, input: PropGetValueArgs<&MainMapping>) -> Option<PropValue> {
        let instance_state = input.control_context.instance_state.borrow();
        let info = instance_state.get_mapping_info(input.object.qualified_id())?;
        Some(PropValue::Text(info.name.clone()))
    }
}

#[derive(Default)]
struct TargetTextValueProp;

impl TargetProp for TargetTextValueProp {
    fn get_value(&self, input: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            input.object.target.text_value(input.control_context)?,
        ))
    }
}

#[derive(Default)]
struct TargetNumericValueProp;

impl TargetProp for TargetNumericValueProp {
    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Numeric(
            args.object.target.numeric_value(args.control_context)?,
        ))
    }
}

#[derive(Default)]
struct TargetNormalizedValueProp;

impl TargetProp for TargetNormalizedValueProp {
    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Normalized(
            args.object
                .target
                .current_value(args.control_context)?
                .to_unit_value(),
        ))
    }
}

#[derive(Default)]
struct TargetTrackIndexProp;

impl TargetProp for TargetTrackIndexProp {
    fn is_affected_by(&self, args: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        matches!(
            args.event,
            CompoundChangeEvent::Reaper(
                ChangeEvent::TrackAdded(_)
                    | ChangeEvent::TrackRemoved(_)
                    | ChangeEvent::TracksReordered(_)
            )
        )
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Index(args.object.target.track()?.index()?))
    }
}

#[derive(Default)]
struct TargetFxIndexProp;

impl TargetProp for TargetFxIndexProp {
    fn is_affected_by(&self, args: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // This could be more specific (taking the track into account) but so what.
        // This doesn't happen that frequently.
        matches!(
            args.event,
            CompoundChangeEvent::Reaper(
                ChangeEvent::FxAdded(_) | ChangeEvent::FxRemoved(_) | ChangeEvent::FxReordered(_)
            )
        )
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Index(args.object.target.fx()?.index()))
    }
}

#[derive(Default)]
struct TargetTrackNameProp;

impl TargetProp for TargetTrackNameProp {
    fn is_affected_by(&self, args: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // This could be more specific (taking the track into account) but so what.
        // This doesn't happen that frequently.
        matches!(args.event, CompoundChangeEvent::Reaper(ChangeEvent::TrackNameChanged(e)) if Some(&e.track) == args.object.target.track())
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(get_track_name(args.object.target.track()?)))
    }
}

#[derive(Default)]
struct TargetNumericValueUnitProp;

impl TargetProp for TargetNumericValueUnitProp {
    fn is_affected_by(&self, _: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // Static in nature (change only when target settings change).
        false
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            args.object
                .target
                .numeric_value_unit(args.control_context)
                .to_string(),
        ))
    }
}

#[derive(Default)]
struct TargetTypeNameProp;

impl TargetProp for TargetTypeNameProp {
    fn is_affected_by(&self, _: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // Static in nature (change only when target settings change).
        false
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            args.object
                .target
                .reaper_target_type()?
                .short_name()
                .to_string(),
        ))
    }
}

#[derive(Default)]
struct TargetTypeLongNameProp;

impl TargetProp for TargetTypeLongNameProp {
    fn is_affected_by(&self, _: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // Static in nature (change only when target settings change).
        false
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            args.object.target.reaper_target_type()?.to_string(),
        ))
    }
}

#[derive(Default)]
struct TargetTrackColorProp;

impl TargetProp for TargetTrackColorProp {
    fn feedback_resolution(
        &self,
        _: PropFeedbackResolutionArgs<MappingAndUnresolvedTarget>,
    ) -> Option<FeedbackResolution> {
        // There are no appropriate change events for this property so we fall back to polling.
        Some(FeedbackResolution::High)
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Color(get_track_color(
            args.object.target.track()?,
        )?))
    }
}

#[derive(Default)]
struct TargetFxNameProp;

// There are no appropriate REAPER change events for this property.
impl TargetProp for TargetFxNameProp {
    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            args.object.target.fx()?.name().into_string(),
        ))
    }
}

#[derive(Default)]
struct TargetRouteIndexProp;

// There are no appropriate REAPER change events for this property.
impl TargetProp for TargetRouteIndexProp {
    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Index(args.object.target.route()?.index()))
    }
}

#[derive(Default)]
struct TargetRouteNameProp;

impl TargetProp for TargetRouteNameProp {
    fn is_affected_by(&self, args: PropIsAffectedByArgs<MappingAndTarget>) -> bool {
        // This could be more specific (taking the route partner into account) but so what.
        // Track names are not changed that frequently.
        matches!(
            args.event,
            CompoundChangeEvent::Reaper(ChangeEvent::TrackNameChanged(_))
        )
    }

    fn get_value(&self, args: PropGetValueArgs<MappingAndTarget>) -> Option<PropValue> {
        Some(PropValue::Text(
            args.object.target.route()?.name().into_string(),
        ))
    }
}
