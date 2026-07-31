use crate::entity::Entity;

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub struct SwappedRow
{
    pub e:    Entity,
    pub from: usize,
    pub to:   usize,
}
