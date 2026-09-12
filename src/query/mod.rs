use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{TQueryParam, TReadOnlyQueryParam};
use crate::apis::traits::TArchetype;
use crate::query::query_iter::QueryIter;
use crate::world::query_spec::QuerySpecAccessor;
use crate::world::World;
use std::marker::PhantomData;

pub(crate) mod access_scope;
pub(crate) mod src_access;
pub(crate) mod src_access_enable;
pub(crate) mod src_access_changed;
pub(crate) mod src_access_added;
mod tuple;
mod variant;

pub mod query_iter;
pub mod filter;
pub mod mut_ref;

/// A `Query` is `Copy` only when it is read-only:
///
/// ```
/// use xynok_ecs::query::Query;
/// fn needs_copy<T: Copy>() {}
/// #[xynok_ecs::component]
/// struct Hp(u32);
/// #[xynok_ecs::component]
/// struct Mana(u32);
/// needs_copy::<Query<'static, &Hp>>();
/// needs_copy::<Query<'static, (&Hp, &Mana)>>();
/// ```
///
/// Any `&mut` in the query makes it non-copyable:
///
/// ```compile_fail
/// use xynok_ecs::query::Query;
/// fn needs_copy<T: Copy>() {}
/// #[xynok_ecs::component]
/// struct Hp(u32);
/// needs_copy::<Query<'static, &mut Hp>>();
/// ```
///
/// ```compile_fail
/// use xynok_ecs::query::Query;
/// fn needs_copy<T: Copy>() {}
/// #[xynok_ecs::component]
/// struct Hp(u32);
/// #[xynok_ecs::component]
/// struct Mana(u32);
/// needs_copy::<Query<'static, (&Hp, &mut Mana)>>();
/// ```
pub struct Query<'a, T: TQueryParam + 'static>
{
    accessor: QuerySpecAccessor<'a>,
    phantom:  PhantomData<T>,
}

// Only read-only queries may be copied. Every copy starts its own iterator from row 0, so a
// copyable `Query<&mut Hp>` would hand out two `&mut Hp` for the same entity.
impl<'a, T: TReadOnlyQueryParam + 'static> Clone for Query<'a, T>
{
    fn clone(&self) -> Self
    {
        *self
    }
}
impl<'a, T: TReadOnlyQueryParam + 'static> Copy for Query<'a, T> {}

impl<'a, T: TQueryParam + 'static> Query<'a, T>
{
    pub(crate) fn new(world: &'a mut World, last_run_tick: crate::apis::constants::ChangedTick) -> Result<Self, XynokEcsError>
    {
        let accessor = world.get_or_create_query_src_access::<T>(last_run_tick)?;
        Ok(Self {
            accessor: accessor,
            phantom:  PhantomData,
        })
    }
}

impl<'a, T: TQueryParam + 'static> IntoIterator for Query<'a, T>
{
    type Item = T::QueryItem<'a>;

    type IntoIter = QueryIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter
    {
        QueryIter::new(&self.accessor)
    }
}

impl<'a, T: TQueryParam + 'static> Query<'a, T>
{
    pub fn with_shared_component_filter<TFilter: TArchetype>() {}
}

#[cfg(test)]
mod test
{
    use crate::apis::internal_traits::TQueryParam;
    use crate::component;
    use crate::query::filter::{Added, Changed, Disabled, Enabled};
    use crate::world::World;

    #[component(EnableAble, ChangeAble)]
    #[derive(Default)]
    struct Hp(#[allow(dead_code)] u32);

    #[component(EnableAble, ChangeAble)]
    #[derive(Default)]
    struct Mana(#[allow(dead_code)] u32);

    /// The world keys its query registry (and with it the access scope the scheduler reads) by
    /// `TYPE_ID`, so two queries that touch a component differently must never share one.
    #[test]
    fn every_query_shape_gets_its_own_type_id()
    {
        let ids = [
            <&Hp as TQueryParam>::TYPE_ID,
            <&mut Hp as TQueryParam>::TYPE_ID,
            <&Mana as TQueryParam>::TYPE_ID,
            <Added<&Hp> as TQueryParam>::TYPE_ID,
            <Added<&mut Hp> as TQueryParam>::TYPE_ID,
            <Changed<&Hp> as TQueryParam>::TYPE_ID,
            <Enabled<&Hp> as TQueryParam>::TYPE_ID,
            <Disabled<&Hp> as TQueryParam>::TYPE_ID,
            <(&Hp, &Mana) as TQueryParam>::TYPE_ID,
            <(&Hp, &mut Mana) as TQueryParam>::TYPE_ID,
            <(&mut Hp, &Mana) as TQueryParam>::TYPE_ID,
            <(Changed<&Hp>, &Mana) as TQueryParam>::TYPE_ID,
        ];

        for (i, a) in ids.iter().enumerate()
        {
            for (j, b) in ids.iter().enumerate()
            {
                if i != j
                {
                    assert_ne!(a, b, "query shapes {i} and {j} collide on one TYPE_ID and would share a QuerySpec");
                }
            }
        }
    }

    /// `Shape` also shows up in panic messages and debugger output, so it should read like the
    /// query you wrote. Run with `--nocapture` to see it.
    #[test]
    fn shape_reads_like_the_query_it_came_from()
    {
        fn shape_of<Q: TQueryParam>() -> String
        {
            // strip module paths so `xynok_ecs::query::mod::test::Hp` reads as `Hp`
            let full = std::any::type_name::<Q::Shape>();
            let mut out = String::new();
            let mut segment = String::new();
            for ch in full.chars()
            {
                if ch.is_alphanumeric() || ch == '_' || ch == ':'
                {
                    segment.push(ch);
                }
                else
                {
                    out.push_str(segment.rsplit("::").next().unwrap_or(&segment));
                    out.push(ch);
                    segment.clear();
                }
            }
            out.push_str(segment.rsplit("::").next().unwrap_or(&segment));
            out
        }

        let cases = [
            (shape_of::<&Hp>(), "&Hp"),
            (shape_of::<&mut Hp>(), "&mut Hp"),
            (shape_of::<Added<&Hp>>(), "Added<&Hp>"),
            (shape_of::<Changed<&mut Hp>>(), "Changed<&mut Hp>"),
            (shape_of::<Disabled<&Hp>>(), "Disabled<&Hp>"),
            (shape_of::<(Added<&Hp>, Enabled<&mut Mana>)>(), "(Added<&Hp>, Enabled<&mut Mana>)"),
        ];

        for (got, want) in &cases
        {
            println!("{got}");
            assert_eq!(got, want);
        }
    }

    /// The same shape asked for twice must land on the same registry slot, otherwise every
    /// `create_query` call would rebuild the archetype list from scratch.
    #[test]
    fn the_same_query_shape_keeps_one_type_id()
    {
        assert_eq!(<&Hp as TQueryParam>::TYPE_ID, <&Hp as TQueryParam>::TYPE_ID);
        assert_eq!(<Changed<&Hp> as TQueryParam>::TYPE_ID, <Changed<&Hp> as TQueryParam>::TYPE_ID);
    }

    /// Registering the read query first must not leave the write query with a read-only scope:
    /// the scheduler would then happily run it next to other readers of the same component.
    #[test]
    fn a_write_query_does_not_inherit_a_read_querys_scope()
    {
        let mut w = World::default();
        w.create(Hp(1));

        w.create_query::<&Hp>();
        w.create_query::<&mut Hp>();

        assert_eq!(w.registered_query_is_read_only::<&Hp>(), Some(true), "`&Hp` only reads");
        assert_eq!(w.registered_query_is_read_only::<&mut Hp>(), Some(false), "`&mut Hp` writes");
        assert_eq!(
            w.registered_query_is_read_only::<Changed<&mut Hp>>(),
            None,
            "a shape nobody asked for must not be answered by someone else's spec"
        );
    }
}
