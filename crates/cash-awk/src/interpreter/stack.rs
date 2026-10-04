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

use super::array::{Key, KeyIterator};
use super::value::{AwkRefType, AwkValue, AwkValueVariant};
use crate::program::{Action, Function, OpCode, SourceLocation};

/// The error for a slot taken as a value that holds none.
const NOT_A_VALUE: &str = "expected a value";

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, PartialEq)]
pub(crate) struct ArrayIterator {
    pub(crate) array: *mut AwkValue,
    pub(crate) iter_var: *mut AwkValue,
    pub(crate) key_iter: KeyIterator,
}

#[cfg_attr(test, derive(Debug))]
#[derive(Clone, PartialEq)]
pub(crate) struct ArrayElementRef {
    pub(crate) array: *mut AwkValue,
    /// The element's key. An index into the array's storage went stale when the
    /// expression using the reference changed the array first: `a["x"] = split(s, a)`
    /// wrote into `a[1]`, and deleting an element on the right panicked
    /// (`REVIEW_REPORT.md` TXT-13). The key is looked up when the reference is used, and
    /// the element made again if it has gone, as gawk has it.
    pub(crate) key: Key,
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
                unsafe { &mut *array_element_ref.array }
                    .as_array()?
                    .get_value(array_element_ref.key.clone())
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
                    let arr = (*array_element_ref.array).as_array()?;
                    let value: *mut AwkValue = arr.get_value(array_element_ref.key.clone())?;
                    Ok(value)
                }
                StackValue::Value(_) => Err("scalar used in array context".to_string()),
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
                Ok(unsafe { &mut *array_element_ref.array }
                    .as_array()?
                    .get_value(array_element_ref.key.clone())?
                    .clone()
                    .into_ref(AwkRefType::None))
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
    pub(crate) sp: *mut StackValue,
    pub(crate) bp: *mut StackValue,
    pub(crate) stack_end: *mut StackValue,
    pub(crate) call_frames: Vec<CallFrame<'i>>,
    pub(crate) _stack_lifetime: PhantomData<&'s ()>,
}

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
        // SAFETY: a popped value's pointers stay valid until the value pushed before it is
        // popped (`push`), and that value is still on the stack.
        unsafe { value.ensure_value_is_scalar()? };
        // SAFETY: as above.
        unsafe { value.into_owned() }
    }

    /// A local to write a scalar to: a parameter still linked to the caller's unused
    /// variable is detached first, into a value of its own, since scalars are passed by
    /// value (see `call_function`).
    pub(crate) fn local_scalar_ptr(&mut self, index: usize) -> Option<*mut AwkValue> {
        if index < self.len() {
            // SAFETY: the index is below `sp`, so the slot is in the stack.
            let slot = unsafe { self.bp.add(index) };
            // SAFETY: a slot below `sp` holds a value (`Stack`'s invariants).
            let slot = unsafe { &mut *slot };
            if let StackValue::UninitializedRef(_) = slot {
                *slot = StackValue::Value(UnsafeCell::new(AwkValue::uninitialized()));
            }
        }
        self.get_mut_value_ptr(index)
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
                StackValue::ArrayElementRef(_) | StackValue::Iterator(_) | StackValue::Invalid => {
                    None
                }
            }
        } else {
            None
        }
    }

    pub(crate) fn pop_value(&mut self) -> Result<AwkValue, String> {
        let value = self.pop().ok_or_else(|| "empty stack".to_string())?;
        // SAFETY: a popped value's pointers stay valid until the value pushed before it is
        // popped (`push`), and that value is still on the stack.
        unsafe { value.into_owned() }
    }

    pub(crate) fn pop_ref(&mut self) -> Result<&mut AwkValue, String> {
        let val = self.pop().ok_or_else(|| "empty stack".to_string())?;
        // SAFETY: as in `pop_value`.
        let ptr = unsafe { val.unwrap_ptr()? };
        // SAFETY: as in `pop_value`.
        Ok(unsafe { &mut *ptr })
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
