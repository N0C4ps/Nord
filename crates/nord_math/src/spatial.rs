//! Bounded, single-writer octree for broad-phase spatial queries.

use crate::{Aabb3, Ray3, Sphere3};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;

const MAX_SUPPORTED_DEPTH: u8 = 64;

/// Configuration for an [`Octree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OctreeConfig {
    leaf_capacity: NonZeroUsize,
    max_depth: u8,
}

impl OctreeConfig {
    /// Configure the number of items retained in a leaf before splitting.
    ///
    /// The depth is capped at 64 to bound recursive traversal and stack use.
    ///
    /// # Errors
    ///
    /// Returns [`OctreeError::DepthTooLarge`] when `max_depth` exceeds 64.
    pub fn new(leaf_capacity: NonZeroUsize, max_depth: u8) -> Result<Self, OctreeError> {
        if max_depth > MAX_SUPPORTED_DEPTH {
            return Err(OctreeError::DepthTooLarge {
                requested: max_depth,
                maximum: MAX_SUPPORTED_DEPTH,
            });
        }
        Ok(Self { leaf_capacity, max_depth })
    }

    /// Maximum number of entries kept in an unsplit leaf.
    #[must_use]
    pub const fn leaf_capacity(self) -> NonZeroUsize {
        self.leaf_capacity
    }

    /// Maximum number of subdivision levels below the root.
    #[must_use]
    pub const fn max_depth(self) -> u8 {
        self.max_depth
    }
}

impl Default for OctreeConfig {
    fn default() -> Self {
        Self { leaf_capacity: NonZeroUsize::new(8).expect("eight is non-zero"), max_depth: 8 }
    }
}

/// Stable identifier assigned to an inserted octree item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OctreeItemId(u64);

impl OctreeItemId {
    /// Numeric identifier, primarily for diagnostics and serialization.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Octree configuration or insertion failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OctreeError {
    /// An item bounds extends outside the fixed root bounds.
    OutsideRoot,
    /// A requested depth exceeds the recursion safety cap.
    DepthTooLarge { requested: u8, maximum: u8 },
    /// No unique ID can be allocated for another item.
    IdentifierExhausted,
}

impl Display for OctreeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutsideRoot => {
                formatter.write_str("item bounds extend outside octree root bounds")
            }
            Self::DepthTooLarge { requested, maximum } => {
                write!(formatter, "octree depth {requested} exceeds supported maximum {maximum}")
            }
            Self::IdentifierExhausted => {
                formatter.write_str("octree item identifiers are exhausted")
            }
        }
    }
}

impl Error for OctreeError {}

struct Entry<T> {
    id: OctreeItemId,
    bounds: Aabb3,
    value: T,
}

struct Node<T> {
    bounds: Aabb3,
    depth: u8,
    entries: Vec<Entry<T>>,
    children: Option<Box<[Self; 8]>>,
}

impl<T> Node<T> {
    fn new(bounds: Aabb3, depth: u8) -> Self {
        Self { bounds, depth, entries: Vec::new(), children: None }
    }

    fn insert(&mut self, entry: Entry<T>, config: OctreeConfig) {
        if let Some(children) = &mut self.children {
            if let Some(index) = child_index(self.bounds, entry.bounds) {
                children[index].insert(entry, config);
            } else {
                self.entries.push(entry);
            }
            return;
        }

        if self.entries.len() < config.leaf_capacity.get() || self.depth >= config.max_depth {
            self.entries.push(entry);
            return;
        }

        self.subdivide();
        let existing = std::mem::take(&mut self.entries);
        for previous in existing {
            self.insert_into_child_or_parent(previous, config);
        }
        self.insert_into_child_or_parent(entry, config);
    }

    fn insert_into_child_or_parent(&mut self, entry: Entry<T>, config: OctreeConfig) {
        let index = child_index(self.bounds, entry.bounds);
        if let (Some(children), Some(index)) = (&mut self.children, index) {
            children[index].insert(entry, config);
        } else {
            self.entries.push(entry);
        }
    }

    fn subdivide(&mut self) {
        let center = self.bounds.center();
        let min = self.bounds.min();
        let max = self.bounds.max();
        self.children = Some(Box::new(std::array::from_fn(|index| {
            let high_x = index & 1 != 0;
            let high_y = index & 2 != 0;
            let high_z = index & 4 != 0;
            let child_min = crate::Vec3::new(
                if high_x { center.x } else { min.x },
                if high_y { center.y } else { min.y },
                if high_z { center.z } else { min.z },
            );
            let child_max = crate::Vec3::new(
                if high_x { max.x } else { center.x },
                if high_y { max.y } else { center.y },
                if high_z { max.z } else { center.z },
            );
            Self::new(
                Aabb3::new(child_min, child_max).expect("subdivision preserves valid bounds"),
                self.depth + 1,
            )
        })));
    }

    fn query_aabb<'a>(&'a self, bounds: Aabb3, result: &mut Vec<&'a Entry<T>>) {
        if !self.bounds.intersects(bounds) {
            return;
        }
        result.extend(self.entries.iter().filter(|entry| entry.bounds.intersects(bounds)));
        if let Some(children) = &self.children {
            for child in children.iter() {
                child.query_aabb(bounds, result);
            }
        }
    }

    fn query_sphere<'a>(&'a self, sphere: Sphere3, result: &mut Vec<&'a Entry<T>>) {
        if !sphere.intersects_aabb(self.bounds) {
            return;
        }
        result.extend(self.entries.iter().filter(|entry| sphere.intersects_aabb(entry.bounds)));
        if let Some(children) = &self.children {
            for child in children.iter() {
                child.query_sphere(sphere, result);
            }
        }
    }

    fn ray_cast<'a>(&'a self, ray: Ray3, result: &mut Vec<(f32, &'a Entry<T>)>) {
        if ray.intersect_aabb(self.bounds).is_none() {
            return;
        }
        result.extend(self.entries.iter().filter_map(|entry| {
            ray.intersect_aabb(entry.bounds).map(|distance| (distance, entry))
        }));
        if let Some(children) = &self.children {
            for child in children.iter() {
                child.ray_cast(ray, result);
            }
        }
    }

    fn remove(&mut self, id: OctreeItemId) -> Option<T> {
        if let Some(index) = self.entries.iter().position(|entry| entry.id == id) {
            return Some(self.entries.swap_remove(index).value);
        }
        self.children.as_mut()?.iter_mut().find_map(|child| child.remove(id))
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.children = None;
    }
}

/// Fixed-root octree supporting AABB, sphere, and ray broad-phase queries.
///
/// The tree is intentionally single-writer and not internally synchronized.
/// Items overlapping multiple octants stay at the common parent; point- and
/// box-queries then remain complete without duplicating items. The root bounds
/// are fixed, so callers rebuilding streamed regions should create a new tree
/// or clear and reuse one with the same spatial domain.
pub struct Octree<T> {
    root: Node<T>,
    config: OctreeConfig,
    len: usize,
    next_id: u64,
}

impl<T> Octree<T> {
    /// Create an empty tree over a fixed world-space region.
    #[must_use]
    pub fn new(root_bounds: Aabb3, config: OctreeConfig) -> Self {
        Self { root: Node::new(root_bounds, 0), config, len: 0, next_id: 1 }
    }

    /// Insert an item whose entire bounds must fit in the root.
    ///
    /// # Errors
    ///
    /// Returns [`OctreeError::OutsideRoot`] when the item bounds exceed the
    /// fixed root bounds, or [`OctreeError::IdentifierExhausted`] if no unique
    /// identifier remains.
    pub fn insert(&mut self, bounds: Aabb3, value: T) -> Result<OctreeItemId, OctreeError> {
        if !self.root.bounds.contains_aabb(bounds) {
            return Err(OctreeError::OutsideRoot);
        }
        let next_id = self.next_id.checked_add(1).ok_or(OctreeError::IdentifierExhausted)?;
        let id = OctreeItemId(self.next_id);
        self.next_id = next_id;
        self.root.insert(Entry { id, bounds, value }, self.config);
        self.len += 1;
        Ok(id)
    }

    /// Remove an item and return its value, if the identifier is present.
    pub fn remove(&mut self, id: OctreeItemId) -> Option<T> {
        let removed = self.root.remove(id);
        if removed.is_some() {
            self.len -= 1;
        }
        removed
    }

    /// Remove all entries while retaining the fixed root and next ID.
    pub fn clear(&mut self) {
        self.root.clear();
        self.len = 0;
    }

    /// Number of live items in the tree.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the tree contains no live items.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Query items whose bounds overlap the supplied AABB.
    #[must_use]
    pub fn query_aabb(&self, bounds: Aabb3) -> Vec<(OctreeItemId, &T)> {
        let mut entries = Vec::new();
        self.root.query_aabb(bounds, &mut entries);
        entries.into_iter().map(|entry| (entry.id, &entry.value)).collect()
    }

    /// Query items whose bounds overlap the supplied sphere.
    #[must_use]
    pub fn query_sphere(&self, sphere: Sphere3) -> Vec<(OctreeItemId, &T)> {
        let mut entries = Vec::new();
        self.root.query_sphere(sphere, &mut entries);
        entries.into_iter().map(|entry| (entry.id, &entry.value)).collect()
    }

    /// Return ray hits sorted from nearest to farthest.
    #[must_use]
    pub fn ray_cast(&self, ray: Ray3) -> Vec<(f32, OctreeItemId, &T)> {
        let mut entries = Vec::new();
        self.root.ray_cast(ray, &mut entries);
        entries.sort_by(|left, right| left.0.total_cmp(&right.0));
        entries.into_iter().map(|(distance, entry)| (distance, entry.id, &entry.value)).collect()
    }

    /// Fixed world-space bounds covered by this tree.
    #[must_use]
    pub const fn root_bounds(&self) -> Aabb3 {
        self.root.bounds
    }
}

fn child_index(parent: Aabb3, item: Aabb3) -> Option<usize> {
    let center = parent.center();
    let minimum = item.min();
    let maximum = item.max();
    let x = if maximum.x <= center.x {
        0
    } else if minimum.x >= center.x {
        1
    } else {
        return None;
    };
    let y = if maximum.y <= center.y {
        0
    } else if minimum.y >= center.y {
        1
    } else {
        return None;
    };
    let z = if maximum.z <= center.z {
        0
    } else if minimum.z >= center.z {
        1
    } else {
        return None;
    };
    Some(x | (y << 1) | (z << 2))
}
