// about to make a ton of rust programmers upset (:

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

    pub fn index_and_before(&self, index:usize) -> Option<(*mut Element, *mut Element)> {
        if index >= self.length {
            return None;
        }
        if index == 0 {
            return None; // this cannot be done
        }
        let mut current = self.bottom;
        for _ in 0..index-1 {
            current = unsafe { (*current.unwrap()).next };
        }
        Some((current.unwrap(), unsafe { (*current.unwrap()).next.unwrap() }))
    }

    pub fn move_to_head (&mut self, index: usize) -> Result<(), String> {
        // get the element at the index
        let elements = self.index_and_before(index);
        if elements.is_none() {
            return Err("index out of bounds".to_string());
        }

        let (before, element) = elements.unwrap();
        unsafe {
            // change top item's next to the element
            (*self.top.unwrap()).next = Some(element);
            // change the before's next to the original element's next
            (*before).next = (*element).next;
            // change the element's next to none
            (*element).next = None;
        }

        Ok(())
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
            return Err("index out of bounds".to_string());
        }

        let elements = self.index_and_before(index);
        if elements.is_none() {
            return Err("index out of bounds".to_string());
        }

        let (before, element) = elements.unwrap();
        unsafe {
            if element == self.top.unwrap() {
                self.top = (*element).next;
            }
            if element == self.bottom.unwrap() {
                self.bottom = (*element).next;
            }
            (*before).next = (*element).next;
            Box::from_raw(element);
        }
        self.length -= 1;
        Ok(())
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