use serde::{Deserialize, Serialize};

use std::{
    collections::{hash_map::RandomState, hash_set::Difference, HashSet},
    fmt::{Debug, Display},
};

use ulid::Ulid;

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy, Deserialize, Serialize)]
pub struct ChunkCoordinates {
    pub x: i32,
    pub y: i32,
}

impl Debug for ChunkCoordinates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for ChunkCoordinates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({},{})", self.x, self.y)
    }
}

impl From<(i32, i32)> for ChunkCoordinates {
    fn from((x, y): (i32, i32)) -> ChunkCoordinates {
        ChunkCoordinates { x, y }
    }
}

#[derive(PartialEq, Clone, Copy, Debug, Deserialize, Serialize)]
pub struct GlobalCoordinates {
    pub x: f32,
    pub y: f32,
}

impl GlobalCoordinates {
    pub fn into_chunk_coordinates(&self) -> ChunkCoordinates {
        ChunkCoordinates {
            x: self.x.floor() as i32,
            y: self.y.floor() as i32,
        }
    }
}

impl From<(f32, f32)> for GlobalCoordinates {
    fn from((x, y): (f32, f32)) -> Self {
        Self { x, y }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Color {
    RGB { r: u8, g: u8, b: u8 },
}

impl Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Self::RGB { r, g, b } => {
                write!(f, "rgb({r},{g},{b})")
            }
        }
    }
}

impl Color {
    pub const fn new_rgb(r: u8, g: u8, b: u8) -> Self {
        Color::RGB { r, g, b }
    }
}

#[derive(PartialEq, Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Width(f32);

impl From<f32> for Width {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

impl Width {
    pub fn new(v: f32) -> Self {
        Self(v)
    }

    pub fn as_f32(&self) -> f32 {
        self.0
    }
}

#[derive(PartialEq, Clone, Copy, Debug, Deserialize, Serialize, Eq, Hash)]
pub struct PathId(Ulid);

impl From<Ulid> for PathId {
    fn from(value: Ulid) -> Self {
        Self(value)
    }
}

#[derive(PartialEq, Clone, Debug, Deserialize, Serialize)]
pub struct Path {
    pub points: Vec<GlobalCoordinates>,
    pub width: Width,
    pub color: Color,
}

impl Path {
    pub fn new(points: Vec<GlobalCoordinates>, width: Width, color: Color) -> Self {
        Self {
            points,
            width,
            color,
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug, Deserialize, Serialize)]
pub enum PathStepAction {
    Draw(PathStepDraw),
    End(PathStepEnd),
}

#[derive(PartialEq, Clone, Copy, Debug, Deserialize, Serialize)]
pub struct PathStepDraw {
    pub id: PathId,
    pub point: GlobalCoordinates,
    pub width: Width,
    pub color: Color,
}

#[derive(PartialEq, Clone, Copy, Debug, Deserialize, Serialize)]
pub struct PathStepEnd {
    pub id: PathId,
}

#[derive(PartialEq, Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Line {
    pub from: GlobalCoordinates,
    pub to: GlobalCoordinates,
    pub width: Width,
    pub color: Color,
}

impl Line {
    pub fn new<C: Into<GlobalCoordinates>>(from: C, to: C, width: Width, color: Color) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            width,
            color,
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Dot {
    pub coordinates: GlobalCoordinates,
    pub diam: Width,
    pub color: Color,
}

impl Dot {
    pub fn new<C: Into<GlobalCoordinates>>(coordinates: C, diam: Width, color: Color) -> Self {
        Self {
            coordinates: coordinates.into(),
            diam,
            color,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Default, Deserialize, Serialize)]
pub struct Subscription {
    pub chunk_coordinates: HashSet<ChunkCoordinates>,
}

impl Subscription {
    pub fn empty() -> Self {
        Self {
            chunk_coordinates: HashSet::new(),
        }
    }

    pub fn contains(&self, chunk_coordinates: &ChunkCoordinates) -> bool {
        self.chunk_coordinates.contains(chunk_coordinates)
    }

    pub fn missing_from_other<'s>(
        &'s self,
        other: &'s Self,
    ) -> Difference<'s, ChunkCoordinates, RandomState> {
        self.chunk_coordinates.difference(&other.chunk_coordinates)
    }

    pub fn missing_from_self<'s>(
        &'s self,
        other: &'s Self,
    ) -> Difference<'s, ChunkCoordinates, RandomState> {
        other.chunk_coordinates.difference(&self.chunk_coordinates)
    }
}

impl Debug for Subscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut chunks: Vec<_> = self.chunk_coordinates.iter().collect();
        chunks.sort();

        f.debug_tuple("Subscription").field(&chunks).finish()
    }
}

impl From<HashSet<ChunkCoordinates>> for Subscription {
    fn from(chunk_coordinates: HashSet<ChunkCoordinates>) -> Self {
        Self { chunk_coordinates }
    }
}

impl<IC: Into<ChunkCoordinates>> From<IC> for Subscription {
    fn from(ic: IC) -> Self {
        Self {
            chunk_coordinates: HashSet::from([ic.into()]),
        }
    }
}

/////////

#[derive(PartialEq, Clone, Debug, Deserialize, Serialize)]
pub enum Message {
    Draw(DrawMessage),
    Subscribe(Subscription),
}

/// Applies to both DrawMessage and any type that implements Into<DrawMessage> (e.g. Line and Dot)
impl<T: Into<DrawMessage>> From<T> for Message {
    fn from(t: T) -> Self {
        Self::Draw(t.into())
    }
}

#[derive(PartialEq, Clone, Debug, Deserialize, Serialize)]
pub enum DrawMessage {
    Composite(CompositeDrawMessage),
    Path(Path),
    PathStepAction(PathStepAction),
    Line(Line),
    Dot(Dot),
}

#[derive(PartialEq, Clone, Debug, Deserialize, Serialize)]
pub struct CompositeDrawMessage(pub Vec<DrawMessage>);

impl From<CompositeDrawMessage> for DrawMessage {
    fn from(composite: CompositeDrawMessage) -> Self {
        Self::Composite(composite)
    }
}

impl From<Vec<DrawMessage>> for DrawMessage {
    fn from(msgs: Vec<DrawMessage>) -> Self {
        Self::Composite(CompositeDrawMessage(msgs))
    }
}

impl From<Path> for DrawMessage {
    fn from(path: Path) -> Self {
        Self::Path(path)
    }
}

impl From<Line> for DrawMessage {
    fn from(line: Line) -> Self {
        Self::Line(line)
    }
}

impl From<Dot> for DrawMessage {
    fn from(dot: Dot) -> Self {
        Self::Dot(dot)
    }
}
