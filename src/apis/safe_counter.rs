pub struct SafeCounter
{
    val:     usize,
    max_val: usize,
    min_val: usize,
}

impl SafeCounter
{
    pub fn new(min: usize, max: usize) -> Self
    {
        Self {
            val:     min,
            min_val: min,
            max_val: max,
        }
    }
    pub fn current_val(&self) -> usize
    {
        self.val
    }
    pub fn increase(&mut self)
    {
        self.val = (self.val + 1).clamp(self.min_val, self.max_val);
    }
}
