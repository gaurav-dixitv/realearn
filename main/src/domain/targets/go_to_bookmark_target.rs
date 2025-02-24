use crate::application::BookmarkAnchorType;
use crate::domain::{
    current_value_of_bookmark, find_bookmark, format_value_as_on_off, AdditionalFeedbackEvent,
    CompoundChangeEvent, ControlContext, ExtendedProcessorContext, FeedbackResolution,
    HitInstructionReturnValue, MappingCompartment, MappingControlContext, RealearnTarget,
    ReaperTarget, ReaperTargetType, TargetCharacter, TargetTypeDef, UnresolvedReaperTargetDef,
    DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, PropValue, RgbColor, Target, UnitValue,
};
use reaper_high::{BookmarkType, ChangeEvent, Project, Reaper};
use reaper_medium::{AutoSeekBehavior, BookmarkRef};
use std::num::NonZeroU32;

#[derive(Debug)]
pub struct UnresolvedGoToBookmarkTarget {
    pub bookmark_type: BookmarkType,
    pub bookmark_anchor_type: BookmarkAnchorType,
    pub bookmark_ref: u32,
    pub set_time_selection: bool,
    pub set_loop_points: bool,
}

impl UnresolvedReaperTargetDef for UnresolvedGoToBookmarkTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        _: MappingCompartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        let project = context.context().project_or_current_project();
        let res = find_bookmark(
            project,
            self.bookmark_type,
            self.bookmark_anchor_type,
            self.bookmark_ref,
        )?;
        Ok(vec![ReaperTarget::GoToBookmark(GoToBookmarkTarget {
            project,
            bookmark_type: self.bookmark_type,
            index: res.index,
            position: NonZeroU32::new(res.index_within_type + 1).unwrap(),
            set_time_selection: self.set_time_selection,
            set_loop_points: self.set_loop_points,
        })])
    }

    fn feedback_resolution(&self) -> Option<FeedbackResolution> {
        Some(FeedbackResolution::Beat)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GoToBookmarkTarget {
    pub project: Project,
    pub bookmark_type: BookmarkType,
    // This counts both markers and regions. We need it for getting the current value.
    pub index: u32,
    // This counts either only markers or only regions. We need it for control. The alternative
    // would be an ID but unfortunately, marker IDs are not unique which means we would
    // unnecessarily lack reliability to go to markers in a position-based way.
    pub position: NonZeroU32,
    pub set_time_selection: bool,
    pub set_loop_points: bool,
}

impl RealearnTarget for GoToBookmarkTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        (
            ControlType::AbsoluteContinuousRetriggerable,
            TargetCharacter::Trigger,
        )
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitInstructionReturnValue, &'static str> {
        if !value.to_unit_value()?.is_zero() {
            match self.bookmark_type {
                BookmarkType::Marker => self
                    .project
                    .go_to_marker(BookmarkRef::Position(self.position)),
                BookmarkType::Region => {
                    self.project
                        .go_to_region_with_smooth_seek(BookmarkRef::Position(self.position));
                    if self.set_loop_points || self.set_time_selection {
                        if let Some(bookmark) = self.project.find_bookmark_by_type_and_index(
                            BookmarkType::Region,
                            self.position.get() - 1,
                        ) {
                            if let Some(end_pos) = bookmark.basic_info.region_end_position {
                                if self.set_loop_points {
                                    self.project.set_loop_points(
                                        bookmark.basic_info.position,
                                        end_pos,
                                        AutoSeekBehavior::DenyAutoSeek,
                                    );
                                }
                                if self.set_time_selection {
                                    self.project
                                        .set_time_selection(bookmark.basic_info.position, end_pos);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.project.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.project)
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        // Handled both from control-surface and non-control-surface callbacks.
        use CompoundChangeEvent::*;
        match evt {
            Reaper(ChangeEvent::BookmarksChanged(e)) if e.project == self.project => (true, None),
            Additional(AdditionalFeedbackEvent::BeatChanged(e)) if e.project == self.project => {
                let v = current_value_of_bookmark(
                    self.project,
                    self.bookmark_type,
                    self.index,
                    e.new_value,
                );
                (true, Some(AbsoluteValue::Continuous(v)))
            }
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<String> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).to_string())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::GoToBookmark)
    }

    fn prop_value(&self, key: &str, _: ControlContext) -> Option<PropValue> {
        match key {
            "bookmark.color" => {
                let res = self
                    .project
                    .find_bookmark_by_type_and_index(self.bookmark_type, self.position.get() - 1)?;
                let reaper_medium::RgbColor { r, g, b } = Reaper::get()
                    .medium_reaper()
                    .color_from_native(res.basic_info.color);
                Some(PropValue::Color(RgbColor::new(r, g, b)))
            }
            _ => None,
        }
    }
}

impl<'a> Target<'a> for GoToBookmarkTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let val = current_value_of_bookmark(
            self.project,
            self.bookmark_type,
            self.index,
            self.project.play_or_edit_cursor_position(),
        );
        Some(AbsoluteValue::Continuous(val))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const GO_TO_BOOKMARK_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Marker/region: Go to",
    short_name: "Go to bookmark",
    ..DEFAULT_TARGET
};
