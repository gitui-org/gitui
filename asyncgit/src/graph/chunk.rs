#[derive(Clone, Debug, PartialEq)]
pub enum Markers {
    Uncommitted,
    Commit,
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub alias: Option<u32>,
    pub parent_a: Option<u32>,
    pub parent_b: Option<u32>,
    pub marker: Markers,
}
