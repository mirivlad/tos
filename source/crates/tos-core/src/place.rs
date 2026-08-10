// SPDX-License-Identifier: GPL-3.0-or-later
//! Places: the paths ownership, borrows and mutation all talk about.
//!
//! docs/40 section 5 states its rules in terms of paths rather than whole
//! bindings: a record may be partially moved, a mutable field borrow locks the
//! containing path but not unrelated fields, and indexed elements count as
//! overlapping unless their indices are compile-time unequal constants.
//!
//! A place is therefore a binding plus a path of field and index steps. Two
//! places interact exactly when one is a prefix of the other, which is what
//! "locks the containing path" means: `p.x` and `p.y` are independent, while
//! `p` and `p.x` are not.

use std::string::String;
use std::vec::Vec;

/// One step of a place path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Segment {
    Field(String),
    /// A constant index, or `None` when the index is not a compile-time
    /// constant. docs/40 section 5 treats an unknown index as overlapping every
    /// element, which is the conservative and deterministic rule.
    Index(Option<i128>),
}

impl Segment {
    /// Whether two steps can name the same location.
    fn may_alias(&self, other: &Segment) -> bool {
        match (self, other) {
            (Segment::Field(left), Segment::Field(right)) => left == right,
            (Segment::Index(Some(left)), Segment::Index(Some(right))) => left == right,
            // A dynamic index may hit any element.
            (Segment::Index(_), Segment::Index(_)) => true,
            // A field step and an index step cannot both apply to one value.
            _ => false,
        }
    }
}

/// A binding and the path walked into it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Place {
    root: BindingId,
    path: Vec<Segment>,
}

/// Identifies one binding occurrence, so shadowing never merges two bindings.
pub(crate) type BindingId = usize;

impl Place {
    pub(crate) fn root(id: BindingId) -> Place {
        Place {
            root: id,
            path: Vec::new(),
        }
    }

    pub(crate) fn binding(&self) -> BindingId {
        self.root
    }

    pub(crate) fn extended(&self, segment: Segment) -> Place {
        let mut path = self.path.clone();
        path.push(segment);
        Place {
            root: self.root,
            path,
        }
    }

    /// Whether `self` names this place or something inside it.
    pub(crate) fn is_prefix_of(&self, other: &Place) -> bool {
        if self.root != other.root || self.path.len() > other.path.len() {
            return false;
        }
        self.path
            .iter()
            .zip(&other.path)
            .all(|(one, another)| one.may_alias(another))
    }

    /// Whether touching one place can affect the other, in either direction.
    pub(crate) fn overlaps(&self, other: &Place) -> bool {
        self.is_prefix_of(other) || other.is_prefix_of(self)
    }

    /// How the place is written in a diagnostic field.
    pub(crate) fn spell(&self, root_name: &str) -> String {
        let mut spelled = String::from(root_name);
        for segment in &self.path {
            match segment {
                Segment::Field(name) => {
                    spelled.push('.');
                    spelled.push_str(name);
                }
                Segment::Index(Some(index)) => {
                    spelled.push_str(&std::format!("[{index}]"));
                }
                Segment::Index(None) => spelled.push_str("[..]"),
            }
        }
        spelled
    }
}
