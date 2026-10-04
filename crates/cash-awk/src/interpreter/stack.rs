//
// Copyright (c) 2024-2026 Hemi Labs, Inc.
//
// This file is part of the posixutils-rs project covered under
// the MIT License.  For the full license text, please see the LICENSE
// file in the root directory of this project.
// SPDX-License-Identifier: MIT
//

use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::rc::Rc;

use super::array::{Array, Key, KeyIterator};
use super::value::{AwkRefType, AwkValue, AwkValueVariant};
use crate::program::{Action, Function, OpCode, SourceLocation};

/// The error for a slot taken as a value that holds none.
const NOT_A_VALUE: &str = "expected a value";

/// A variable, or an element of an array: what a `for (k in a)` loop goes through and
/// assigns to, and what an error names.
#[cfg_attr(test, derive(Debug))]
#[derive(Clone, PartialEq)]
pub(crate) enum Place {
    Variable(*mut AwkValue),
    Element(ArrayElementRef),
}

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, PartialEq)]
pub(crate) struct ArrayIterator {
    pub(crate) array: Place,
    pub(crate) iter_var: Place,
    pub(crate) key_iter: KeyIterator,
}

/// An element of an array, by its keys: `a[k]` is the element `k` of the variable `a`'s
/// array, and `a[1][k]` the element `k` of the subarray `a[1]`. A subarray is the element
/// that holds it, so a reference to `a[1]` stands for the subarray as well.
///
/// The element is found from the variable each time the reference is used, which a
/// pointer into the array's storage would not survive: the expression that uses the
/// reference can change the array first. An index went stale so, and `a["x"] = split(s,
/// a)` wrote into `a[1]`, and deleting an element on the right panicked
/// (`REVIEW_REPORT.md` TXT-13); a subarray, which moves when its array grows, is the same.
/// The element is made again if it has gone, as gawk has it.
#[cfg_attr(test, derive(Debug))]
#[derive(Clone, PartialEq)]
pub(crate) struct ArrayElementRef {
    /// The variable whose array holds the element, directly or through subarrays: a
    /// global, a local, or the caller's variable a parameter refers to, each of which
    /// outlives the reference.
    pub(crate) array: *mut AwkValue,
    /// The keys of the subarrays between the variable's array and the element's: none for
    /// `a[k]`, `1` for `a[1][k]`.
    pub(crate) path: Option<Rc<[Key]>>,
    pub(crate) key: Key,
}

/// Why an element could not be found: the error, and what it is about, for its name.
pub(crate) struct ElementError {
    pub(crate) error: String,
    pub(crate) place: Place,
}

impl ArrayElementRef {
    /// The element `key` of the subarray this reference stands for.
    pub(crate) fn subscript(&self, key: Key) -> Self {
        let path: Vec<Key> = self.keys().cloned().collect();
        Self {
            array: self.array,
            path: Some(path.into()),
            key,
        }
    }

    /// Every key, the element's last.
    pub(crate) fn keys(&self) -> impl Iterator<Item = &Key> {
        self.path
            .iter()
            .flat_map(|path| path.iter())
            .chain(std::iter::once(&self.key))
    }

    /// The reference to the subarray at `depth` on the way, 0 being the variable's
    /// array's element.
    fn on_the_way(&self, depth: usize) -> Self {
        let path = self.path.as_deref().unwrap_or_default();
        Self {
            array: self.array,
            path: (depth > 0).then(|| path.iter().take(depth).cloned().collect()),
            key: path.get(depth).cloned().unwrap_or_else(|| self.key.clone()),
        }
    }

    /// The array the element is in. With `make`, the subarrays on the way are made where
    /// they are missing, and an element with no type yet becomes one; without, `None` says
    /// one is missing or has no type, so that the element is not there.
    ///
    /// # Safety
    /// `array` has to be valid and dereferencable
    pub(crate) unsafe fn container(&self, make: bool) -> Result<Option<*mut Array>, ElementError> {
        // SAFETY: the caller upholds this function's `# Safety` contract.
        let root = unsafe { &mut *self.array };
        if !make && matches!(root.value, AwkValueVariant::Uninitialized) {
            return Ok(None);
        }
        let mut array = root.as_array().map_err(|error| ElementError {
            error,
            place: Place::Variable(self.array),
        })?;
        for (depth, key) in self.path.iter().flat_map(|path| path.iter()).enumerate() {
            let value = if make {
                array.get_value(key.clone()).map_err(|error| ElementError {
                    error,
                    place: Place::Element(self.on_the_way(depth)),
                })?
            } else {
                match array.existing_value(key) {
                    Some(value) if !matches!(value.value, AwkValueVariant::Uninitialized) => value,
                    _ => return Ok(None),
                }
            };
            array = value.as_array().map_err(|error| ElementError {
                error,
                place: Place::Element(self.on_the_way(depth)),
            })?;
        }
        Ok(Some(array as *mut Array))
    }

    /// The element, made if it is not there, with the subarrays on the way.
    ///
    /// # Safety
    /// `array` has to be valid and dereferencable
    pub(crate) unsafe fn element(&self) -> Result<*mut AwkValue, ElementError> {
        // SAFETY: the caller upholds this function's `# Safety` contract.
        let container = unsafe { self.container(true) }?;
        let container = container.ok_or_else(|| ElementError {
            error: "array element vanished".to_string(),
            place: Place::Element(self.clone()),
        })?;
        // SAFETY: the array is in the variable's, valid as the variable is.
        let container = unsafe { &mut *container };
        let element: *mut AwkValue =
            container
                .get_value(self.key.clone())
                .map_err(|error| ElementError {
                    error,
                    place: Place::Element(self.clone()),
                })?;
        Ok(element)
    }

    /// The element, if it is there; none is made.
    ///
    /// # Safety
    /// `array` has to be valid and dereferencable
    pub(crate) unsafe fn existing_element(&self) -> Result<Option<*mut AwkValue>, ElementError> {
        // SAFETY: the caller upholds this function's `# Safety` contract.
        let container = unsafe { self.container(false) }?;
        // SAFETY: the array is in the variable's, valid as the variable is.
        let container = container.map(|array| unsafe { &mut *array });
        Ok(container
            .and_then(|array| array.existing_value(&self.key))
            .map(|element| element as *mut AwkValue))
    }
}

pub(crate) enum StackValue {
    Value(UnsafeCell<AwkValue>),
    ValueRef(*mut AwkValue),
    ArrayElementRef(ArrayElementRef),
    UninitializedRef(*mut AwkValue),
    Iterator(ArrayIterator),
    Invalid,
}

impl StackValue {
    /// The value this is or refers to. An iterator, or a slot left empty, is no value:
    /// one taken as a value would be malformed code, an error rather than a panic.
    ///
    /// # Safety
    /// the caller has to ensure that the value is valid and dereferencable
    pub(crate) unsafe fn value_ref(&mut self) -> Result<&mut AwkValue, String> {
        match self {
            StackValue::Value(val) => Ok(val.get_mut()),
            // SAFETY: the caller ensures the pointer is valid (`# Safety`).
            StackValue::ValueRef(val_ref) => Ok(unsafe { &mut **val_ref }),
            // SAFETY: the caller ensures the pointer is valid (`# Safety`).
            StackValue::UninitializedRef(val_ref) => Ok(unsafe { &mut **val_ref }),
            // An element reference is made from an array, and a variable that is an array
            // stays one; `get_value` inserts a key it does not find.
            StackValue::ArrayElementRef(array_element_ref) => {
                // SAFETY: the caller ensures the array pointer is valid (`# Safety`).
                let element = unsafe { array_element_ref.element() }.map_err(|e| e.error)?;
                // SAFETY: as above; the element is in the variable's array.
                Ok(unsafe { &mut *element })
            }
            StackValue::Iterator(_) | StackValue::Invalid => Err(NOT_A_VALUE.to_string()),
        }
    }

    /// # Safety
    /// if the `StackValue` is an `ArrayElementRef`, the caller has to ensure that the
    /// array is dereferencable
    pub(crate) unsafe fn unwrap_ptr(self) -> Result<*mut AwkValue, String> {
        // SAFETY: the caller upholds this function's `# Safety` contract.
        unsafe {
            match self {
                StackValue::ValueRef(ptr) => Ok(ptr),
                StackValue::UninitializedRef(ptr) => Ok(ptr),
                StackValue::ArrayElementRef(array_element_ref) => {
                    array_element_ref.element().map_err(|e| e.error)
                }
                StackValue::Value(_) => Err(SCALAR_IN_ARRAY_CONTEXT.to_string()),
                _ => Err("expected lvalue".to_string()),
            }
        }
    }

    /// The iterator a `for (k in a)` loop keeps on the stack; anything else there is
    /// malformed code, an error rather than a panic.
    pub(crate) fn unwrap_array_iterator(self) -> Result<ArrayIterator, String> {
        match self {
            StackValue::Iterator(array_iterator) => Ok(array_iterator),
            _ => Err("expected an array iterator".to_string()),
        }
    }

    /// The value this is or refers to, owned; an error for no value, as in `value_ref`.
    ///
    /// # Safety
    /// pointers inside the `StackValue` have to be valid and dereferencable
    pub(crate) unsafe fn into_owned(self) -> Result<AwkValue, String> {
        match self {
            StackValue::Value(val) => Ok(val.into_inner()),
            StackValue::ValueRef(ref_val) => {
                // SAFETY: the caller ensures the pointer is valid (`# Safety`).
                Ok(unsafe { &*ref_val }.clone().into_ref(AwkRefType::None))
            }
            StackValue::UninitializedRef(_) => Ok(AwkValue::uninitialized_scalar()),
            // As in `value_ref`.
            StackValue::ArrayElementRef(array_element_ref) => {
                // SAFETY: the caller ensures the array pointer is valid (`# Safety`).
                let element = unsafe { array_element_ref.element() }.map_err(|e| e.error)?;
                // SAFETY: as above; the element is in the variable's array.
                Ok(unsafe { &*element }.clone().into_ref(AwkRefType::None))
            }
            StackValue::Iterator(_) | StackValue::Invalid => Err(NOT_A_VALUE.to_string()),
        }
    }

    /// # Safety
    /// pointers inside the `StackValue` have to be valid and dereferencable
    pub(crate) unsafe fn ensure_value_is_scalar(&mut self) -> Result<(), String> {
        // SAFETY: the caller upholds this function's `# Safety` contract.
        unsafe { self.value_ref()?.ensure_value_is_scalar() }
    }

    /// # Safety
    /// `value` has to be a valid pointer at least until the value preceding it
    /// on the stack is popped
    pub(crate) unsafe fn from_var(value: *mut AwkValue) -> Self {
        // SAFETY: the caller upholds this function's `# Safety` contract.
        unsafe {
            let value_ref = &mut *value;
            match value_ref.value {
                AwkValueVariant::Array(_) => StackValue::ValueRef(value),
                AwkValueVariant::Uninitialized => StackValue::UninitializedRef(value),
                _ => StackValue::Value(UnsafeCell::new(value_ref.clone())),
            }
        }
    }

    pub(crate) fn duplicate(&mut self) -> Self {
        match self {
            StackValue::Value(val) => val.get_mut().clone().into(),
            StackValue::ValueRef(val_ref) => StackValue::ValueRef(*val_ref),
            StackValue::UninitializedRef(uninitialized_ref) => {
                StackValue::UninitializedRef(*uninitialized_ref)
            }
            StackValue::ArrayElementRef(array_element_ref) => {
                StackValue::ArrayElementRef(array_element_ref.clone())
            }
            StackValue::Iterator(iterator) => StackValue::Iterator(iterator.clone()),
            StackValue::Invalid => StackValue::Invalid,
        }
    }
}

impl From<AwkValue> for StackValue {
    fn from(value: AwkValue) -> Self {
        StackValue::Value(UnsafeCell::new(value))
    }
}

pub(crate) struct CallFrame<'i> {
    pub(crate) function_name: Rc<str>,
    pub(crate) function_file: Rc<str>,
    pub(crate) source_locations: &'i [SourceLocation],
    pub(crate) array_names: &'i [Option<Rc<str>>],
    pub(crate) parameter_names: &'i [Rc<str>],
    pub(crate) bp: *mut StackValue,
    pub(crate) sp: *mut StackValue,
    pub(crate) ip: isize,
    pub(crate) instructions: &'i [OpCode],
}

/// # Invariants
/// - `sp` and `bp` are pointers into the same
///   contiguously allocated chunk of memory
/// - `stack_end` is one past the last valid pointer
///   of the allocated memory starting at `bp`
/// - values in the range [`bp`, `sp`) can be accessed safely
pub(crate) struct Stack<'i, 's> {
    pub(crate) current_function_name: Rc<str>,
    pub(crate) current_function_file: Rc<str>,
    pub(crate) ip: isize,
    pub(crate) instructions: &'i [OpCode],
    pub(crate) source_locations: &'i [SourceLocation],
    /// The current code's `DebugInfo::array_names`.
    pub(crate) array_names: &'i [Option<Rc<str>>],
    /// The current function's parameters' names; none outside a function.
    pub(crate) parameter_names: &'i [Rc<str>],
    /// The variable or element that an error is about, when the error found it: an array
    /// read as a scalar, or a scalar taken by reference as an array.
    /// `Interpreter::fatal_message` names it.
    pub(crate) error_place: Option<Place>,
    /// The instruction that ran last, in the current code; -1 before one has run.
    pub(crate) last_ip: isize,
    pub(crate) sp: *mut StackValue,
    pub(crate) bp: *mut StackValue,
    pub(crate) stack_end: *mut StackValue,
    pub(crate) call_frames: Vec<CallFrame<'i>>,
    pub(crate) _stack_lifetime: PhantomData<&'s ()>,
}

/// The error of an array read as a scalar, and of a scalar used as an array, before the
/// interpreter puts them in gawk's words with the variable's name.
pub(crate) const ARRAY_IN_SCALAR_CONTEXT: &str = "array used in scalar context";
pub(crate) const SCALAR_IN_ARRAY_CONTEXT: &str = "scalar used in array context";

/// Safe interface to work with the program stack.
impl<'i, 's> Stack<'i, 's> {
    /// pops the `StackValue` on top of the stack.
    /// # Returns
    /// The top stack value if there is one. `None` otherwise
    pub(crate) fn pop(&mut self) -> Option<StackValue> {
        if self.sp != self.bp {
            let mut value = StackValue::Invalid;
            // SAFETY: `sp` is above `bp`, so the slot below it is in the stack (`Stack`'s
            // invariants).
            self.sp = unsafe { self.sp.sub(1) };
            // SAFETY: `sp` is in [`bp`, `stack_end`), a slot of the stack's slice, which is
            // initialized; it is left holding `Invalid`.
            unsafe { core::ptr::swap(&mut value, self.sp) };
            Some(value)
        } else {
            None
        }
    }

    /// pushes a StackValue on top of the stack
    /// # Errors
    /// returns an error in case of stack overflow
    /// # Safety
    /// `value` has to be valid at least until the value preceding it is popped
    pub(crate) unsafe fn push(&mut self, value: StackValue) -> Result<(), String> {
        if self.sp == self.stack_end {
            Err("stack overflow".to_string())
        } else {
            // SAFETY: `sp` is below `stack_end`, so it is a slot of the stack's slice, which
            // is initialized; the value it held is dropped.
            unsafe { *self.sp = value };
            // SAFETY: one past a slot of the stack is at most `stack_end`, one past the end
            // of the slice.
            self.sp = unsafe { self.sp.add(1) };
            Ok(())
        }
    }

    pub(crate) fn pop_scalar_value(&mut self) -> Result<AwkValue, String> {
        let mut value = self.pop().ok_or_else(|| "empty stack".to_string())?;
        if let StackValue::ArrayElementRef(element) = &value {
            let element = self.scalar_element(element)?;
            return Ok(element.clone().into_ref(AwkRefType::None));
        }
        // SAFETY: a popped value's pointers stay valid until the value pushed before it is
        // popped (`push`), and that value is still on the stack.
        if let Err(error) = unsafe { value.ensure_value_is_scalar() } {
            // An array on the stack is a variable's, by reference (`from_var`).
            if let StackValue::ValueRef(variable) = value {
                self.error_place = Some(Place::Variable(variable));
            }
            return Err(error);
        }
        // SAFETY: as above.
        unsafe { value.into_owned() }
    }

    /// The element `element` refers to, made if it is not there; an error names what
    /// was wrong.
    pub(crate) fn element_ptr(
        &mut self,
        element: &ArrayElementRef,
    ) -> Result<*mut AwkValue, String> {
        // SAFETY: a reference's variable outlives the reference (`ArrayElementRef`), which
        // was on the stack until now.
        unsafe { element.element() }.map_err(|error| {
            self.error_place = Some(error.place);
            error.error
        })
    }

    /// The element `element` refers to, which must be a scalar: one with no type yet
    /// becomes one.
    fn scalar_element(&mut self, element: &ArrayElementRef) -> Result<&mut AwkValue, String> {
        let ptr = self.element_ptr(element)?;
        // SAFETY: the element is in the variable's array, which outlives the reference.
        let value = unsafe { &mut *ptr };
        if let Err(error) = value.ensure_value_is_scalar() {
            self.error_place = Some(Place::Element(element.clone()));
            return Err(error);
        }
        Ok(value)
    }

    /// The variable a reference on top of the stack refers to, popped, which must hold a
    /// scalar: the target of an assignment, `++` or `getline var`.
    pub(crate) fn pop_scalar_ref(&mut self) -> Result<&mut AwkValue, String> {
        let val = self.pop().ok_or_else(|| "empty stack".to_string())?;
        if let StackValue::ArrayElementRef(element) = &val {
            return self.scalar_element(element);
        }
        // SAFETY: as in `pop_scalar_value`.
        let ptr = unsafe { val.unwrap_ptr()? };
        // SAFETY: as in `pop_scalar_value`.
        let value = unsafe { &mut *ptr };
        if let Err(error) = value.ensure_value_is_scalar() {
            self.error_place = Some(Place::Variable(ptr));
            return Err(error);
        }
        Ok(value)
    }

    /// The array a reference on top of the stack refers to, popped: the operand of `in`,
    /// `delete` and `split`. A subarray that is a scalar is named in the error only with
    /// `name_element`: gawk names it for some uses and not for others.
    pub(crate) fn pop_array(&mut self, name_element: bool) -> Result<&mut Array, String> {
        let val = self.pop().ok_or_else(|| "empty stack".to_string())?;
        let (ptr, place) = match val {
            StackValue::ArrayElementRef(element) => {
                let ptr = self.element_ptr(&element)?;
                (ptr, name_element.then_some(Place::Element(element)))
            }
            val => {
                // SAFETY: as in `pop_scalar_value`.
                let ptr = unsafe { val.unwrap_ptr()? };
                (ptr, Some(Place::Variable(ptr)))
            }
        };
        // SAFETY: as in `pop_scalar_value`.
        let value = unsafe { &mut *ptr };
        if !value.can_be_array() {
            self.error_place = place;
            return Err(SCALAR_IN_ARRAY_CONTEXT.to_string());
        }
        value.as_array()
    }

    /// The reference to the element `key` of the array that the value on top of the stack
    /// is or refers to, popped: a variable's, or a subarray. Nothing is looked up yet.
    pub(crate) fn pop_element_ref(&mut self, key: Key) -> Result<ArrayElementRef, String> {
        match self.pop().ok_or_else(|| "empty stack".to_string())? {
            StackValue::ValueRef(array) | StackValue::UninitializedRef(array) => {
                Ok(ArrayElementRef {
                    array,
                    path: None,
                    key,
                })
            }
            StackValue::ArrayElementRef(element) => Ok(element.subscript(key)),
            // A variable that holds a scalar is pushed as a copy (`from_var`); the error
            // names it from the code (`DebugInfo::array_names`).
            StackValue::Value(_) => Err(SCALAR_IN_ARRAY_CONTEXT.to_string()),
            StackValue::Iterator(_) | StackValue::Invalid => Err(NOT_A_VALUE.to_string()),
        }
    }

    /// The array or the scalar on top of the stack, popped, for `length` and `isarray`.
    /// A variable with no type yet becomes a scalar with `make_scalar`, as `length`
    /// makes it in gawk; `isarray` leaves it, as an element with none is left by both.
    /// An array was copied whole to be counted.
    pub(crate) fn pop_array_or_scalar(
        &mut self,
        make_scalar: bool,
    ) -> Result<Result<usize, AwkValue>, String> {
        let val = self.pop().ok_or_else(|| "empty stack".to_string())?;
        let ptr = match &val {
            StackValue::ArrayElementRef(element) => self.element_ptr(element)?,
            StackValue::ValueRef(ptr) => *ptr,
            StackValue::UninitializedRef(ptr) => {
                if make_scalar {
                    // SAFETY: as in `pop_scalar_value`.
                    unsafe { &mut **ptr }.value = AwkValueVariant::UninitializedScalar;
                }
                return Ok(Err(AwkValue::uninitialized()));
            }
            // SAFETY: as in `pop_scalar_value`.
            _ => return unsafe { val.into_owned() }.map(Err),
        };
        // SAFETY: as in `pop_scalar_value`.
        let value = unsafe { &*ptr };
        Ok(match &value.value {
            AwkValueVariant::Array(array) => Ok(array.len()),
            _ => Err(value.clone().into_ref(AwkRefType::None)),
        })
    }

    /// What local `index` holds when it is an element of a caller's array, a parameter
    /// given a subarray or an element with no type yet (`f(a[1])`).
    pub(crate) fn local_element(&self, index: usize) -> Option<ArrayElementRef> {
        match self.slot(self.bp, self.sp, index)? {
            StackValue::ArrayElementRef(element) => Some(element.clone()),
            _ => None,
        }
    }

    /// Slot `index` of the frame whose values are [`bp`, `sp`), if it has one.
    fn slot(&self, bp: *mut StackValue, sp: *mut StackValue, index: usize) -> Option<&StackValue> {
        // SAFETY: a frame's `bp` and `sp` point into the stack's slice, `sp` not below `bp`.
        let len = unsafe { sp.offset_from(bp) }.unsigned_abs();
        if index >= len {
            return None;
        }
        // SAFETY: the index is below the frame's `sp`, so the slot is in the stack.
        let slot = unsafe { bp.add(index) };
        // SAFETY: the slot is below the current `sp`, and holds a value (`Stack`'s
        // invariants).
        Some(unsafe { &*slot })
    }

    /// Each call frame's values and its parameters' names, the current frame's first and
    /// its callers' after it, innermost first.
    pub(crate) fn frames(
        &self,
    ) -> impl Iterator<Item = (*mut StackValue, *mut StackValue, &'i [Rc<str>])> + '_ {
        std::iter::once((self.bp, self.sp, self.parameter_names)).chain(
            self.call_frames
                .iter()
                .rev()
                .map(|frame| (frame.bp, frame.sp, frame.parameter_names)),
        )
    }

    /// The parameter of the frame at [`bp`, `sp`) that `holds` is true of, by name.
    pub(crate) fn parameter_where(
        &self,
        (bp, sp, names): (*mut StackValue, *mut StackValue, &'i [Rc<str>]),
        holds: impl Fn(&StackValue) -> bool,
    ) -> Option<Rc<str>> {
        names.iter().enumerate().find_map(|(index, name)| {
            self.slot(bp, sp, index)
                .filter(|slot| holds(slot))
                .map(|_| name.clone())
        })
    }

    /// The reference to write a scalar to local `index`: a parameter linked to a caller's
    /// variable or element with no type yet is detached first, into a value of its own,
    /// since scalars are passed by value (see `call_function`). One that is a caller's
    /// subarray stays the reference, for the error to name it.
    pub(crate) fn local_scalar_ref(&mut self, index: usize) -> Option<StackValue> {
        if let Some(element) = self.local_element(index) {
            // SAFETY: a parameter's element is of a variable that outlives the call.
            if let Ok(Some(ptr)) = unsafe { element.existing_element() } {
                // SAFETY: as above.
                let value = unsafe { &mut *ptr };
                match value.value {
                    AwkValueVariant::Array(_) => {
                        return Some(StackValue::ArrayElementRef(element));
                    }
                    // The caller's element becomes a scalar, as in gawk.
                    AwkValueVariant::Uninitialized => {
                        value.value = AwkValueVariant::UninitializedScalar;
                    }
                    _ => {}
                }
            }
            // SAFETY: the index is below `sp`, as `local_element` found it.
            let slot = unsafe { self.bp.add(index) };
            // SAFETY: a slot below `sp` holds a value (`Stack`'s invariants).
            let slot = unsafe { &mut *slot };
            *slot = StackValue::Value(UnsafeCell::new(AwkValue::uninitialized()));
        }
        self.local_scalar_ptr(index).map(StackValue::ValueRef)
    }

    /// A local to write a scalar to: a parameter still linked to the caller's unused
    /// variable is detached first, into a value of its own, since scalars are passed by
    /// value (see `call_function`). The caller's variable becomes a scalar, as in gawk,
    /// where it was left free to become an array.
    pub(crate) fn local_scalar_ptr(&mut self, index: usize) -> Option<*mut AwkValue> {
        if index < self.len() {
            // SAFETY: the index is below `sp`, so the slot is in the stack.
            let slot = unsafe { self.bp.add(index) };
            // SAFETY: a slot below `sp` holds a value (`Stack`'s invariants).
            let slot = unsafe { &mut *slot };
            if let StackValue::UninitializedRef(variable) = slot {
                // SAFETY: the caller's variable outlives the call.
                let variable = unsafe { &mut **variable };
                if matches!(variable.value, AwkValueVariant::Uninitialized) {
                    variable.value = AwkValueVariant::UninitializedScalar;
                }
                *slot = StackValue::Value(UnsafeCell::new(AwkValue::uninitialized()));
            }
        }
        self.get_mut_value_ptr(index)
    }

    /// Whether the current call frame has no values on the stack.
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many values the current call frame has on the stack.
    fn len(&self) -> usize {
        // SAFETY: `sp` and `bp` point into the same slice, and `sp` is not below `bp`
        // (`Stack`'s invariants).
        let len = unsafe { self.sp.offset_from(self.bp) };
        len.unsigned_abs()
    }

    /// A pointer to local `index` of the current call frame, `None` past its values or
    /// where the slot holds no variable, which would be malformed code. A local at `sp`
    /// itself, one past the last value, was taken for one (`>=` where `>` belonged),
    /// reading a slot that holds no value (`REVIEW_REPORT.md` ARCH-01).
    pub(crate) fn get_mut_value_ptr(&mut self, index: usize) -> Option<*mut AwkValue> {
        if index < self.len() {
            // SAFETY: the index is below `sp`, so the slot is in the stack.
            let value = unsafe { self.bp.add(index) };
            // SAFETY: a slot below `sp` holds a value (`Stack`'s invariants).
            let value = unsafe { &*value };
            match value {
                StackValue::Value(val) => Some(val.get()),
                StackValue::ValueRef(val_ref) => Some(*val_ref),
                StackValue::UninitializedRef(val_ref) => Some(*val_ref),
                // An element is found by `local_element`.
                StackValue::ArrayElementRef(_) | StackValue::Iterator(_) | StackValue::Invalid => {
                    None
                }
            }
        } else {
            None
        }
    }

    /// The local of the current frame that `variable` is, and whether the local refers to
    /// it (a caller's variable passed in) rather than holding it, for an error's name.
    pub(crate) fn local_holding(&self, variable: *const AwkValue) -> Option<(usize, bool)> {
        (0..self.len()).find_map(|index| {
            // SAFETY: the index is below `sp`, so the slot is in the stack.
            let slot = unsafe { self.bp.add(index) };
            // SAFETY: a slot below `sp` holds a value (`Stack`'s invariants).
            let slot = unsafe { &*slot };
            match slot {
                StackValue::Value(cell) if cell.get().cast_const() == variable => {
                    Some((index, false))
                }
                StackValue::ValueRef(ptr) | StackValue::UninitializedRef(ptr)
                    if ptr.cast_const() == variable =>
                {
                    Some((index, true))
                }
                _ => None,
            }
        })
    }

    pub(crate) fn push_value<V: Into<AwkValue>>(&mut self, value: V) -> Result<(), String> {
        // SAFETY: a `StackValue::Value` holds no pointer, so it is valid for as long as it
        // is on the stack.
        unsafe { self.push(StackValue::from(value.into())) }
    }

    /// pushes a reference on the stack
    /// # Safety
    /// `value_ptr` has to be safe to access at least until the value preceding it
    /// on the stack is popped.
    pub(crate) unsafe fn push_ref(&mut self, value_ptr: *mut AwkValue) -> Result<(), String> {
        // SAFETY: the caller upholds this function's `# Safety` contract.
        unsafe { self.push(StackValue::ValueRef(value_ptr)) }
    }

    /// The program's file and the line of the current instruction.
    pub(crate) fn location(&self) -> Option<(Rc<str>, u32)> {
        let location = self.source_locations.get(usize::try_from(self.ip).ok()?)?;
        Some((self.current_function_file.clone(), location.line))
    }

    pub(crate) fn next_instruction(&mut self) -> Option<OpCode> {
        self.instructions.get(self.ip as usize).copied()
    }

    /// Enter `function`, whose parameters are the frame's last values. The compiler
    /// pushes one for each parameter, the missing ones too; fewer would be malformed
    /// code, an error rather than a panic.
    pub(crate) fn call_function(&mut self, function: &'i Function) -> Result<(), String> {
        if self.len() < function.parameters_count {
            return Err("function called without its parameters".to_string());
        }
        // A variable the caller has not used yet stays linked to the caller's: if the
        // function uses it as an array, the caller's variable becomes that array, as in
        // every awk. A scalar is passed by value instead, so the first scalar write
        // detaches the parameter (`local_scalar_ptr`). The link used to be cut at the
        // call, so `function f(a) { a[1] = 5 } BEGIN { f(arr); print arr[1] }` printed
        // nothing (`REVIEW_REPORT.md` TXT-18).
        // SAFETY: the frame holds the parameters, so `sp` less their count is not below
        // `bp`.
        let new_bp = unsafe { self.sp.sub(function.parameters_count) };
        let caller_frame = CallFrame {
            bp: self.bp,
            sp: new_bp,
            ip: self.ip,
            instructions: self.instructions,
            source_locations: self.source_locations,
            array_names: self.array_names,
            parameter_names: self.parameter_names,
            function_file: self.current_function_file.clone(),
            function_name: self.current_function_name.clone(),
        };
        self.current_function_file = function.debug_info.file.clone();
        self.current_function_name = function.name.clone();
        self.call_frames.push(caller_frame);
        self.bp = new_bp;
        self.ip = 0;
        self.instructions = &function.instructions;
        self.source_locations = &function.debug_info.source_locations;
        self.array_names = &function.debug_info.array_names;
        self.parameter_names = &function.parameter_names;
        Ok(())
    }

    /// Return to the caller's frame. The compiler allows `return` only in a function,
    /// so there is always one; the error stands in for what was a panic.
    pub(crate) fn restore_caller(&mut self) -> Result<(), String> {
        let caller_frame = self
            .call_frames
            .pop()
            .ok_or_else(|| "return outside of a function".to_string())?;
        self.bp = caller_frame.bp;
        self.sp = caller_frame.sp;
        self.instructions = caller_frame.instructions;
        self.ip = caller_frame.ip;
        // The caller's debug information too: an error after the call was placed by the
        // callee's source locations, and named in its function.
        self.source_locations = caller_frame.source_locations;
        self.array_names = caller_frame.array_names;
        self.parameter_names = caller_frame.parameter_names;
        self.current_function_file = caller_frame.function_file;
        self.current_function_name = caller_frame.function_name;
        Ok(())
    }

    pub(crate) fn new(main: &'i Action, stack: &'s mut [StackValue]) -> Self {
        let stack_len = stack.len();
        let bp = stack.as_mut_ptr();
        // SAFETY: one past the end of the slice is in bounds for `add`.
        let stack_end = unsafe { bp.add(stack_len) };
        Self {
            current_function_file: main.debug_info.file.clone(),
            current_function_name: "<start>".into(),
            instructions: &main.instructions,
            source_locations: &main.debug_info.source_locations,
            array_names: &main.debug_info.array_names,
            parameter_names: &[],
            error_place: None,
            last_ip: -1,
            ip: 0,
            bp,
            sp: bp,
            stack_end,
            call_frames: Vec::new(),
            _stack_lifetime: PhantomData,
        }
    }
}

pub(crate) enum ExecutionResult {
    Expression(AwkValue),
    Next,
    NextFile,
    Exit(i32),
}

impl ExecutionResult {
    /// The truth of a pattern's value, or, as the error, what a function the pattern
    /// called did instead: `next`, `nextfile` or `exit`.
    pub(crate) fn pattern_matches(self) -> Result<bool, Self> {
        match self {
            ExecutionResult::Expression(value) => Ok(value.scalar_as_bool()),
            other => Err(other),
        }
    }

    #[cfg(test)]
    #[expect(
        clippy::panic,
        reason = "a test that expects a value fails loudly without one"
    )]
    pub(crate) fn unwrap_expr(self) -> AwkValue {
        match self {
            ExecutionResult::Expression(value) => value,
            _ => panic!("execution result is not an expression"),
        }
    }
}

macro_rules! numeric_op {
    ($stack:expr, $op:tt) => {
        let rhs = $stack.pop_scalar_value()?.scalar_as_f64();
        let lhs = $stack.pop_scalar_value()?.scalar_as_f64();
        $stack.push_value(lhs $op rhs)?;
    };
}

macro_rules! compare_op {
    ($stack:expr, $convfmt:expr, $op:tt) => {
        let rhs = $stack.pop_scalar_value()?;
        let lhs = $stack.pop_scalar_value()?;
        match (&lhs.value, &rhs.value) {
            (AwkValueVariant::Number(lhs), AwkValueVariant::Number(rhs)) => {
                $stack.push_value(bool_to_f64(lhs $op rhs))?;
            }
            (AwkValueVariant::String(lhs), AwkValueVariant::String(rhs)) => {
              	if lhs.is_numeric && rhs.is_numeric {
									$stack.push_value(bool_to_f64(strtod(lhs) $op strtod(rhs)))?;
              	} else {
                	$stack.push_value(bool_to_f64(lhs.as_str() $op rhs.as_str()))?;
              	}
            }
            (AwkValueVariant::UninitializedScalar, AwkValueVariant::UninitializedScalar) => {
                $stack.push_value(bool_to_f64(0.0 $op 0.0))?;
            }
            // POSIX 85481: an uninitialized value (including a nonexistent or
            // empty field) compared with a number is compared numerically.
            (AwkValueVariant::Number(n), AwkValueVariant::UninitializedScalar) => {
                $stack.push_value(bool_to_f64(*n $op 0.0))?;
            }
            (AwkValueVariant::UninitializedScalar, AwkValueVariant::Number(n)) => {
                $stack.push_value(bool_to_f64(0.0 $op *n))?;
            }
            (AwkValueVariant::String(s), AwkValueVariant::Number(x)) if s.is_numeric => {
                $stack.push_value(bool_to_f64(lhs.scalar_as_f64() $op *x))?;
            }
            (AwkValueVariant::Number(x), AwkValueVariant::String(s)) if s.is_numeric => {
                $stack.push_value(bool_to_f64(*x $op rhs.scalar_as_f64()))?;
            }
            (_, _) => {
                $stack.push_value(bool_to_f64(lhs.scalar_to_string($convfmt)?.as_str() $op rhs.scalar_to_string($convfmt)?.as_str()))?;
            }
        }
    };
}

pub(crate) use compare_op;
pub(crate) use numeric_op;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_past_the_last_value_is_none() {
        // `>=` took the slot at `sp` for a local, which holds no value, and the read
        // reached `unreachable!` (REVIEW_REPORT.md ARCH-01).
        let action = Action {
            debug_info: Default::default(),
            instructions: Vec::new(),
        };
        let mut slots: Vec<StackValue> = std::iter::repeat_with(|| StackValue::Invalid)
            .take(4)
            .collect();
        let mut stack = Stack::new(&action, &mut slots);
        stack.push_value(1.0).unwrap();
        assert!(stack.get_mut_value_ptr(0).is_some());
        assert!(stack.get_mut_value_ptr(1).is_none());
        assert!(stack.local_scalar_ptr(1).is_none());
    }
}
