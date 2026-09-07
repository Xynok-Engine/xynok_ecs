use crate::system::traits::SystemTypeStorage;

pub enum ScheduleStep
{
    Single(SystemTypeStorage),
    Parallel(Vec<SystemTypeStorage>),
}
