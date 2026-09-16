use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{QueryTicks, TQueryParam, TReadOnlyQueryParam};
use crate::apis::params::ComponentSpecs;
use crate::chunk::layout::ChunkLayout;
use crate::entity::Entity;
use crate::query::access_scope::AccessScope;

/// The handle of the row being walked, e.g. `Query<(&Entity, &mut Hp)>`.
///
/// `Entity` is not a component. Every chunk keeps its handles in the header, so this reads from
/// there and claims no component id: the scope stays empty and never collides with anyone.
/// On its own, `Query<&Entity>` therefore matches every archetype.
///
/// A handle is read-only, so there is no `&mut Entity`. Filters do not apply to it either, since
/// it has no state region.
impl TQueryParam for &Entity
{
    type QueryItem<'a> = &'a Entity;
    // offset of the entity region in the chunk
    type ArchState = usize;
    type Fetch = *mut u8;
    type Shape = &'static Entity;

    fn access_scope(_component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        Ok(AccessScope::default())
    }

    #[inline]
    fn arch_state(layout: &ChunkLayout) -> usize
    {
        layout.header.entities_offset
    }

    #[inline]
    unsafe fn fetch_init(state: usize, chunk_ptr: *mut u8) -> *mut u8
    {
        unsafe { chunk_ptr.add(state) }
    }

    #[inline]
    unsafe fn accepts(_fetch: &*mut u8, _row: usize, _ticks: QueryTicks) -> bool
    {
        true
    }

    #[inline]
    unsafe fn fetch<'a>(fetch: &*mut u8, row: usize, _ticks: QueryTicks) -> &'a Entity
    {
        unsafe { &*(*fetch as *const Entity).add(row) }
    }
}
// SAFETY: only `&'a Entity` is handed out, nothing can be written through it
unsafe impl TReadOnlyQueryParam for &Entity {}

#[cfg(test)]
mod test
{
    use crate::component;
    use crate::entity::Entity;
    use crate::query::filter::Without;
    use crate::world::World;

    #[component]
    struct Hp(u32);

    #[component]
    struct Frozen;

    #[test]
    fn entity_comes_with_its_own_row()
    {
        let mut w = World::default();
        let a = w.create(Hp(1));
        let b = w.create(Hp(2));
        let c = w.create((Hp(3), Frozen));

        let mut got: Vec<(Entity, u32)> = w.create_query::<(&Entity, &mut Hp)>().into_iter().map(|(e, hp)| (*e, hp.0)).collect();
        got.sort_by_key(|x| x.1);
        assert_eq!(got, vec![(a, 1), (b, 2), (c, 3)]);

        let got: Vec<Entity> = w.create_query::<(&Entity, Without<Frozen>)>().into_iter().map(|(e, _)| *e).collect();
        assert_eq!(got.len(), 2);
        assert!(!got.contains(&c));
    }

    #[test]
    fn entity_alone_walks_every_archetype()
    {
        let mut w = World::default();
        let a = w.create(Hp(1));
        let b = w.create((Hp(2), Frozen));
        let mut got: Vec<Entity> = w.create_query::<&Entity>().into_iter().copied().collect();
        got.sort_by_key(|e| e.idx());
        assert_eq!(got, vec![a, b]);
    }
}
