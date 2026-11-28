//! A pure-Rust port of the [open-bot-avatar] engine by Dominik Scholz (MIT, commit `e10afe4`):
//! avatar definitions and validation, the implied volume, the motion model (states, moods,
//! gaze, blinking, ribbons, marks), the per-frame animator, flat frames as plain data, and a
//! tiny-skia painter that mirrors the browser's canvas painter for the flat material.
//!
//! Frames are numerically identical to the TypeScript engine's (golden tests in `tests/`),
//! including the SVG path text of every outline, with [`Tuning::Upstream`]. The default
//! [`Tuning::Solar`] only changes the ribbon colours (see [`Tuning`]).
//!
//! ```no_run
//! use bot_avatar::{example, paint, AvatarState, PreparedAvatar, RenderOptions};
//!
//! let def = example("mug").unwrap();
//! let avatar = PreparedAvatar::new(&def, None).unwrap();
//! let frame = avatar.frame(&RenderOptions {
//!     color: Some("#1fbf68".into()),
//!     state: Some(AvatarState::Working.into()),
//!     time: Some(1.2),
//!     size: Some(64.0),
//!     ..RenderOptions::default()
//! });
//! let pixmap = paint(&frame, 128);
//! ```
//!
//! [open-bot-avatar]: https://github.com/DominikScholz/open-bot-avatar

pub mod animator;
pub mod color;
pub mod contour;
pub mod distance;
pub mod effects;
pub mod examples;
pub mod eyes;
pub mod gallery;
pub mod geom;
pub mod jsmath;
pub mod marks;
pub mod motion;
pub mod paint;
pub mod path;
pub mod render;
pub mod seams;
pub mod validate;
pub mod volume;

pub use animator::{
    follow_gaze, follow_step, mood_progress, pointer_gaze, settle_mood, Animator, AnimatorInput,
    AnimatorStep, FollowState, Rect, MOOD_MS,
};
pub use examples::{example, examples};
pub use gallery::{
    definition_json, gallery, gallery_avatar, gallery_color, solar_avatar, solar_avatars,
};
pub use geom::{BoxF, Matrix, Outline, PathCmd};
pub use marks::{AvatarMark, MarkContext, MarkPart, PoseMark};
pub use motion::{
    blend_poses, compute_pose, working_compact, AvatarMood, AvatarMotion, AvatarState, BlendFrom,
    EyeLook, Gaze, MotionBlend, Pose, PoseInput, PosePatch, State, StateContext, StateDefinition,
    View, BASE_PITCH, BASE_YAW, DONE_CYCLE_S, DONE_TURN_S, MOODS, STATES,
};
pub use paint::{fit_square, frame_placement, paint, paint_into, CanvasSurface, PaintParams};
pub use render::{
    avatar_extent, avatar_frame, live_frame, prepare, AvatarFrame, EyeMark, FramePath, FrameShape,
    FrameSurface, Gradient, LiveTransform, PreparedAvatar, RenderOptions, RibbonShape, ShapeKey,
    Square, SurfaceEye, Tuning, ViewBasis, DEFAULT_COLOR, DEFAULT_DEPTH, DEFAULT_EYE_COLOR,
};
pub use validate::{
    validate_avatar, validate_definition, AvatarBody, AvatarDefinition, AvatarEyes,
};
