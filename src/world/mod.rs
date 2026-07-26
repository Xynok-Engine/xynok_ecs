use std::{any::TypeId, collections::HashMap};

use crate::chunk::Chunk;
mod archetype;

pub struct World
{
    archetypes: HashMap<TypeId, Vec<Chunk>>,
}
