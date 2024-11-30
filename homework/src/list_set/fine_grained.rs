use std::cmp::Ordering::*;
use std::sync::{Mutex, MutexGuard};
use std::{mem, ptr};

use crate::ConcurrentSet;

#[derive(Debug)]
struct Node<T> {
    data: T,
    next: Mutex<*mut Node<T>>,
}

/// Concurrent sorted singly linked list using fine-grained lock-coupling.
#[derive(Debug)]
pub struct FineGrainedListSet<T> {
    head: Mutex<*mut Node<T>>,
}

unsafe impl<T: Send> Send for FineGrainedListSet<T> {}
unsafe impl<T: Send> Sync for FineGrainedListSet<T> {}

/// Reference to the `next` field of previous node which points to the current node.
///
/// For example, given the following linked list:
///
/// ```text
/// head -> 1 -> 2 -> 3 -> null
/// ```
///
/// If `cursor` is currently at node 2, then `cursor.0` should be the `MutexGuard` obtained from the
/// `next` of node 1. In particular, `cursor.0.as_ref().unwrap()` creates a shared reference to node
/// 2.
struct Cursor<'l, T>(MutexGuard<'l, *mut Node<T>>);

impl<T> Node<T> {
    fn new(data: T, next: *mut Self) -> *mut Self {
        Box::into_raw(Box::new(Self {
            data,
            next: Mutex::new(next),
        }))
    }
}

impl<T: Ord> Cursor<'_, T> {
    /// Moves the cursor to the position of key in the sorted list.
    /// Returns whether the value was found.
    fn find(&mut self, key: &T) -> bool {
        // 返回最后一个小于等于key的节点
        if let Some(mut first_node) = unsafe { self.0.as_mut() } {
            let mut first_key = &first_node.data;
            if let Equal = key.cmp(first_key) {
                return true;
            }
            let mut next = first_node.next.lock().unwrap();
            while let Some(mut next_node) = unsafe { next.as_mut() } {
                let mut next_key = &next_node.data;
                match key.cmp(next_key) {
                    Equal => {
                        drop(next);
                        return true;
                    }
                    Less => {
                        // self.0是否会自动释放锁？
                        self.0 = next;
                        next = next_node.next.lock().unwrap();
                    }
                    Greater => {
                        drop(next);
                        break;
                    }
                }
            }
        }
        false
    }
}

impl<T> FineGrainedListSet<T> {
    /// Creates a new list.
    pub fn new() -> Self {
        Self {
            head: Mutex::new(ptr::null_mut()),
        }
    }
}

impl<T: Ord> FineGrainedListSet<T> {
    fn find(&self, key: &T) -> (bool, Cursor<'_, T>) {
        let mut c = Cursor(self.head.lock().unwrap());
        let found = c.find(key);
        (found, c)
    }
}

impl<T: Ord> ConcurrentSet<T> for FineGrainedListSet<T> {
    fn contains(&self, key: &T) -> bool {
        self.find(key).0
    }

    fn insert(&self, key: T) -> bool {
        let (found, mut cur) = self.find(&key);
        if !found {
            // cur在目标位置之前一个节点
            match unsafe{cur.0.as_mut()}{
                Some(prev) => {
                    let mut prev_next = prev.next.lock().unwrap();
                    let new_node = Node::new(key, *prev_next);
                    *prev_next = new_node;
                }
                None => {
                    let new_node = Node::new(key, ptr::null_mut());
                    *cur.0 = new_node;
                }
            }
            return true;
        }
        return false;
    }

    fn remove(&self, key: &T) -> bool {
        let first = self.head.lock().unwrap();
        if first.is_null() {
            return false;
        }
        let first_node = unsafe { first.as_mut().unwrap() };
        if *key == first_node.data {}
        let (found, cur) = self.find(&key);
        if found {
            // cur在目标位置之前一个节点
            let prev = unsafe { cur.0.as_mut().unwrap() };
            let mut target = *prev.next.lock().unwrap();
            // 释放目标节点的内存
            let b = unsafe { Box::from_raw(target) };
            // 下下个节点
            let next = b.next.lock().unwrap();
            let mut p = prev.next.lock().unwrap();
            *p = *next;
            return true;
        }
        return false;
    }
}

#[derive(Debug)]
pub struct Iter<'l, T> {
    cursor: MutexGuard<'l, *mut Node<T>>,
}

impl<T> FineGrainedListSet<T> {
    /// An iterator visiting all elements.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            cursor: self.head.lock().unwrap(),
        }
    }
}

impl<'l, T> Iterator for Iter<'l, T> {
    type Item = &'l T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor.is_null() {
            return None;
        }
        if let Some(node) = unsafe { self.cursor.as_mut() } {
            let data = &node.data;
            self.cursor = node.next.lock().unwrap();
            return Some(data);
        }
        None
    }
}

impl<T> Drop for FineGrainedListSet<T> {
    fn drop(&mut self) {
        let mut head = *self.head.lock().unwrap();
        while !head.is_null() {
            let mut node = unsafe { Box::from_raw(head) };
            head = *node.next.lock().unwrap();
            drop(node);
        }
    }
}

impl<T> Default for FineGrainedListSet<T> {
    fn default() -> Self {
        Self::new()
    }
}
