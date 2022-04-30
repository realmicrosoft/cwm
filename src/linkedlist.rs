// about to make a ton of rust programmers upset (:

use std::fmt::format;
use crate::types;

pub struct Element {
    pub(crate) value: types::CumWindow,
    next: Option<*mut Element>
}

pub struct LinkedList {
    bottom: Option<*mut Element>,
    top: Option<*mut Element>,
    length: usize
}

impl LinkedList {
    pub fn new() -> Self {
        LinkedList {
            bottom: None,
            top: None,
            length: 0
        }
    }

    pub fn index(&self, index: usize) -> Option<*mut Element> {
        if index >= self.length {
            return None;
        }

        let mut current = self.bottom;
        for _ in 0..index {
            current = unsafe { (*current.unwrap()).next };
        }

        current
    }

    pub fn index_and_before(&self, index:usize) -> (Option<*mut Element>, Option<*mut Element>) {
        let mut at_index: Option<*mut Element> = None;
        let mut before_index: Option<*mut Element> = None;
        if index >= self.length {
            at_index = None;
            before_index = None;
        }
        if index == 0 {
            at_index = self.bottom;
            before_index = None;
        }
        if before_index.is_some() {
            return (unsafe{(*before_index.unwrap()).next}, before_index);
        } else {
            let mut current = self.bottom;
            for _ in 0..index - 1 {
                current = unsafe { (*current.unwrap()).next };
            }
            return (unsafe{(*current.unwrap()).next}, current);
        }
    }

    pub fn move_to_head (&mut self, index: usize) -> Result<(), String> {
        // get the element at the index
        let elements = self.index_and_before(index);

        // if before isn't found, we're at the bottom; thus, we don't need to change the previous element
        if elements.1.is_none() && elements.0.is_some() {
            unsafe {
                // change top item's next to the element
                (*self.top.unwrap()).next = elements.0;
                // change the bottom to the element's next
                self.bottom = (*elements.0.unwrap()).next;
                // change the element's next to none
                (*elements.0.unwrap()).next = None;
            }
            Ok(())
        } else if elements.0.is_none() { // index doesn't exist, return error
            return Err(format!("index {} doesn't exist", index));
        } else {
            unsafe {
                // change top item's next to the element
                (*self.top.unwrap()).next = elements.0;
                // change the before's next to the original element's next
                (*elements.1.unwrap()).next = (*elements.0.unwrap()).next;
                // change the element's next to none
                (*elements.0.unwrap()).next = None;
            }

            Ok(())
        }
    }

    pub fn push(&mut self, value: types::CumWindow) -> Result<(), String> {
        let mut new_element = Box::new(Element {
            value: value,
            next: None
        });
        let new_element_ptr = Box::into_raw(new_element);
        unsafe {
            if self.bottom.is_none() {
                self.bottom = Some(new_element_ptr);
                self.top = Some(new_element_ptr);
            } else {
                (*self.top.unwrap()).next = Some(new_element_ptr);
                self.top = Some(new_element_ptr);
            }
        }
        self.length += 1;
        Ok(())
    }

    pub fn remove_at_index(&mut self, index: usize) -> Result<(), String> {
        if index >= self.length {
            return Err(format!("index out of bounds: {}", index));
        }

        let elements = self.index_and_before(index);
        // if before isn't found, we're at the bottom; thus, just deallocate the element and set the bottom to the next element
        if elements.1.is_none() && elements.0.is_some() {
            unsafe {
                Box::from_raw(elements.0.unwrap());
                self.bottom = (*elements.0.unwrap()).next;
            }
            self.length -= 1;
            Ok(())
        } else if elements.0.is_none() { // index doesn't exist, return error
            return Err(format!("index {} doesn't exist", index));
        } else {
            unsafe {
                Box::from_raw(elements.0.unwrap());
                (*elements.1.unwrap()).next = (*elements.0.unwrap()).next;
            }
            self.length -= 1;
            Ok(())
        }
    }

    pub fn next_element(&self, element: *mut Element) -> Option<*mut Element> {
        unsafe {
            (*element).next
        }
    }

    pub fn len(&self) -> usize {
        self.length
    }
}