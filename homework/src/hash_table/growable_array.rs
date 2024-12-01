//! Growable array.

use core::fmt::Debug;
use core::mem::{self, ManuallyDrop};
use core::sync::atomic::Ordering::*;

use crossbeam_epoch::{Atomic, Guard, Owned, Shared};

/// Growable array of `Atomic<T>`.
///
/// This is more complete version of the dynamic sized array from the paper. In the paper, the
/// segment table is an array of arrays (segments) of pointers to the elements. In this
/// implementation, a segment contains the pointers to the elements **or other child segments**. In
/// other words, it is a tree that has segments as internal nodes.
///
/// # Example run
///
/// Suppose `SEGMENT_LOGSIZE = 3` (segment size 8).
///
/// When a new `GrowableArray` is created, `root` is initialized with `Atomic::null()`.
///
/// ```text
///                          +----+
///                          |root|
///                          +----+
/// ```
///
/// When you store element `cat` at the index `0b001`, it first initializes a segment.
///
/// ```text
///                          +----+
///                          |root|
///                          +----+
///                            | height: 1
///                            v
///                 +---+---+---+---+---+---+---+---+
///                 |111|110|101|100|011|010|001|000|
///                 +---+---+---+---+---+---+---+---+
///                                           |
///                                           v
///                                         +---+
///                                         |cat|
///                                         +---+
/// ```
///
/// When you store `fox` at `0b111011`, it is clear that there is no room for indices larger than
/// `0b111`. So it first allocates another segment for upper 3 bits and moves the previous root
/// segment (`0b000XXX` segment) under the `0b000XXX` branch of the the newly allocated segment.
///
/// ```text
///                          +----+
///                          |root|
///                          +----+
///                            | height: 2
///                            v
///                 +---+---+---+---+---+---+---+---+
///                 |111|110|101|100|011|010|001|000|
///                 +---+---+---+---+---+---+---+---+
///                                               |
///                                               v
///                                      +---+---+---+---+---+---+---+---+
///                                      |111|110|101|100|011|010|001|000|
///                                      +---+---+---+---+---+---+---+---+
///                                                                |
///                                                                v
///                                                              +---+
///                                                              |cat|
///                                                              +---+
/// ```
///
/// And then, it allocates another segment for `0b111XXX` indices.
///
/// ```text
///                          +----+
///                          |root|
///                          +----+
///                            | height: 2
///                            v
///                 +---+---+---+---+---+---+---+---+
///                 |111|110|101|100|011|010|001|000|
///                 +---+---+---+---+---+---+---+---+
///                   |                           |
///                   v                           v
/// +---+---+---+---+---+---+---+---+    +---+---+---+---+---+---+---+---+
/// |111|110|101|100|011|010|001|000|    |111|110|101|100|011|010|001|000|
/// +---+---+---+---+---+---+---+---+    +---+---+---+---+---+---+---+---+
///                   |                                            |
///                   v                                            v
///                 +---+                                        +---+
///                 |fox|                                        |cat|
///                 +---+                                        +---+
/// ```
///
/// Finally, when you store `owl` at `0b000110`, it traverses through the `0b000XXX` branch of the
/// height 2 segment and arrives at its `0b110` leaf.
///
/// ```text
///                          +----+
///                          |root|
///                          +----+
///                            | height: 2
///                            v
///                 +---+---+---+---+---+---+---+---+
///                 |111|110|101|100|011|010|001|000|
///                 +---+---+---+---+---+---+---+---+
///                   |                           |
///                   v                           v
/// +---+---+---+---+---+---+---+---+    +---+---+---+---+---+---+---+---+
/// |111|110|101|100|011|010|001|000|    |111|110|101|100|011|010|001|000|
/// +---+---+---+---+---+---+---+---+    +---+---+---+---+---+---+---+---+
///                   |                        |                   |
///                   v                        v                   v
///                 +---+                    +---+               +---+
///                 |fox|                    |owl|               |cat|
///                 +---+                    +---+               +---+
/// ```
///
/// When the array is dropped, only the segments are dropped and the **elements must not be
/// dropped/deallocated**.
///
/// ```text
///                 +---+                    +---+               +---+
///                 |fox|                    |owl|               |cat|
///                 +---+                    +---+               +---+
/// ```
///
/// Instead, it should be handled by the container that the elements actually belong to. For
/// example, in `SplitOrderedList` the destruction of elements are handled by the inner `List`.
#[derive(Debug)]
pub struct GrowableArray<T> {
    root: Atomic<Segment<T>>,
    height: usize,
}

const SEGMENT_LOGSIZE: usize = 10;

/// A fixed size array of atomic pointers to other `Segment<T>` or `T`.
///
/// Each segment is either a child segment with pointers to `Segment<T>` or an element segment with
/// pointers to `T`. This is determined by the height of this segment in the main array, which one
/// needs to track separately. For example, use the main array root's tag.
///
/// Since destructing `Segment<T>` requires its height information, it is not recommended to
/// implement `Drop` for this union. Rather, have a custom deallocate method that accounts for the
/// height of the segment.
union Segment<T> {
    children: ManuallyDrop<[Atomic<Segment<T>>; 1 << SEGMENT_LOGSIZE]>,
    elements: ManuallyDrop<[Atomic<T>; 1 << SEGMENT_LOGSIZE]>,
}

impl<T> Segment<T> {
    /// Create a new segment filled with null pointers. It is up to the callee to whether to use
    /// this as a children or an element segment.
    fn new() -> Owned<Self> {
        Owned::new(
            // SAFETY: An array of null pointers can be interperted as either an element segment or
            // a children segment.
            unsafe { mem::zeroed() },
        )
    }
}

impl<T> Debug for Segment<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Segment")
    }
}

impl<T> Drop for GrowableArray<T> {
    /// Deallocate segments, but not the individual elements.
    fn drop(&mut self) {
        todo!()
    }
}

impl<T> Default for GrowableArray<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> GrowableArray<T> {
    /// Create a new growable array.
    pub fn new() -> Self {
        Self {
            height: 0,
            root: Atomic::null(),
        }
    }

    /// Returns the reference to the `Atomic` pointer at `index`. Allocates new segments if
    /// necessary.
    pub fn get<'g>(&mut self, mut index: usize, guard: &'g Guard) -> &'g Atomic<T> {
        // current_index需要正确初始化：我们需要index的多少位？
        // 如果index超出边界，树高需要增加1，至少要index的(height+1)*SEGMENT_LOGSIZE个low bit
        let mut current_index = index;
        let mut parent = &self.root;
        let mut current_shared = parent.load(SeqCst, guard);
        let mut current_node = unsafe { current_shared.as_ref() };
        loop {
            match current_node {
                // 需要申请一个新节点
                None => {
                    let new_node = Segment::<T>::new();
                    // 将新节点插入到树中
                    match parent.compare_exchange(current_shared, new_node, SeqCst, SeqCst, &guard)
                    {
                        Ok(new_shared) => {
                            // todo current_index需要更新
                            return unsafe {
                                &new_shared.as_ref().unwrap().elements[current_index]
                            };
                        }
                        Err(err) => {
                            panic!("compare_exchange failed: {:?}", err);
                        }
                    }
                }
                // 找到了节点
                Some(node) => {
                    // case1: index在当前segment中
                    // return unsafe { &node.elements[current_index] };
                    // case2: index在子segment中
                    // 更新parent, current_shared, current_node, current_index
                    parent = unsafe { &node.children[current_index] };
                    current_shared = parent.load(SeqCst, guard);
                    current_node = unsafe { current_shared.as_ref() };
                }
            }
        }
    }
}
