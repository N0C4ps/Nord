//! Game-owned demand planning and asynchronous flat-terrain chunk streaming.

use super::{FlatTerrainBlockIds, VoxelWorld, generate_flat_terrain_chunk};
use nord_app::{
    ApplicationAabb3 as Aabb3, ApplicationFrustum3 as Frustum3, ApplicationTaskExecutionError,
    ApplicationTaskHandle, ApplicationTaskPriority, ApplicationTaskScheduler,
    ApplicationVec3 as Vec3,
};
use nord_world::{BlockPosition, CHUNK_SIZE, Chunk, ChunkPosition};
use std::collections::{HashMap, HashSet};

const HORIZONTAL_LOAD_RADIUS: i32 = 8;
const HORIZONTAL_UNLOAD_MARGIN: i32 = 2;
const VERTICAL_LOAD_RADIUS: i32 = 2;
const VERTICAL_UNLOAD_MARGIN: i32 = 1;
const MAX_SUBMISSIONS_PER_UPDATE: usize = 8;

/// World-height bounds and load policy for the current flat-world streamer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkStreamConfig {
    chunks_above_origin: i32,
    chunks_below_origin: i32,
    horizontal_load_radius: i32,
    horizontal_unload_margin: i32,
    vertical_load_radius: i32,
    vertical_unload_margin: i32,
    max_submissions_per_update: usize,
}

impl ChunkStreamConfig {
    /// Build a streamer configuration from vertical chunk counts around Y=0.
    ///
    /// `chunks_above_origin` includes chunk Y=0. `chunks_below_origin` sets
    /// the inclusive lower chunk coordinate to its negated value.
    #[must_use]
    pub const fn new(chunks_above_origin: i32, chunks_below_origin: i32) -> Self {
        Self {
            chunks_above_origin: if chunks_above_origin < 1 { 1 } else { chunks_above_origin },
            chunks_below_origin: if chunks_below_origin < 1 { 1 } else { chunks_below_origin },
            horizontal_load_radius: HORIZONTAL_LOAD_RADIUS,
            horizontal_unload_margin: HORIZONTAL_UNLOAD_MARGIN,
            vertical_load_radius: VERTICAL_LOAD_RADIUS,
            vertical_unload_margin: VERTICAL_UNLOAD_MARGIN,
            max_submissions_per_update: MAX_SUBMISSIONS_PER_UPDATE,
        }
    }

    /// Set horizontal load radius in chunks. The streamer unloads outside a
    /// radius two chunks larger to prevent boundary thrashing.
    #[must_use]
    pub const fn with_horizontal_load_radius(mut self, radius: i32) -> Self {
        self.horizontal_load_radius = if radius < 0 {
            0
        } else if radius > 128 {
            128
        } else {
            radius
        };
        self
    }

    /// Set how many chunk layers around the focus are generated vertically.
    #[must_use]
    pub const fn with_vertical_load_radius(mut self, radius: i32) -> Self {
        self.vertical_load_radius = if radius < 0 {
            0
        } else if radius > 128 {
            128
        } else {
            radius
        };
        self
    }

    /// Limit newly queued generation work per update.
    #[must_use]
    pub const fn with_max_submissions_per_update(mut self, maximum: usize) -> Self {
        self.max_submissions_per_update = if maximum == 0 {
            1
        } else if maximum > 256 {
            256
        } else {
            maximum
        };
        self
    }

    const fn minimum_chunk_y(self) -> i32 {
        -self.chunks_below_origin
    }

    const fn maximum_chunk_y(self) -> i32 {
        self.chunks_above_origin - 1
    }
}

/// A compact snapshot of streaming activity for diagnostics and UI overlays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChunkStreamSnapshot {
    pub focus_chunk: Option<ChunkPosition>,
    pub desired_chunks: usize,
    pub loaded_chunks: usize,
    pub generation_jobs: usize,
    pub generated_this_update: usize,
    pub unloaded_this_update: usize,
    pub visible_desired_chunks: usize,
    pub failed_jobs_this_update: usize,
    pub rejected_jobs_this_update: usize,
}

#[derive(Debug, Clone, Copy)]
struct Candidate {
    position: ChunkPosition,
    priority: ApplicationTaskPriority,
    distance_squared: i64,
    visible: bool,
}

/// Asynchronous chunk demand planner owned by the game.
///
/// It keeps worker jobs bounded through Nord's priority scheduler, generates
/// chunks off-thread, integrates completed chunks on the caller thread, and
/// rejects results that are already beyond the unload horizon.
pub struct ChunkStreamer {
    config: ChunkStreamConfig,
    block_ids: FlatTerrainBlockIds,
    focus_chunk: Option<ChunkPosition>,
    priority_frustum: Option<Frustum3>,
    candidates: Vec<Candidate>,
    desired: HashSet<ChunkPosition>,
    in_flight:
        HashMap<ChunkPosition, ApplicationTaskHandle<Result<Chunk, ApplicationTaskExecutionError>>>,
    last_snapshot: ChunkStreamSnapshot,
}

impl ChunkStreamer {
    /// Create a streamer using IDs from the game's validated block registry.
    #[must_use]
    pub fn new(config: ChunkStreamConfig, world: &VoxelWorld) -> Self {
        Self {
            config,
            block_ids: world.terrain_block_ids(),
            focus_chunk: None,
            priority_frustum: None,
            candidates: Vec::new(),
            desired: HashSet::new(),
            in_flight: HashMap::new(),
            last_snapshot: ChunkStreamSnapshot::default(),
        }
    }

    /// Reconcile wanted chunks around the focus and advance asynchronous work.
    ///
    /// The view frustum changes scheduling priority only; the wider radius
    /// remains loaded so turning the camera does not cause terrain pop-in.
    pub fn update(
        &mut self,
        world: &mut VoxelWorld,
        scheduler: &ApplicationTaskScheduler,
        focus: BlockPosition,
        frustum: Frustum3,
    ) -> ChunkStreamSnapshot {
        let focus_chunk = focus.chunk();
        let focus_changed = self.focus_chunk != Some(focus_chunk);
        if focus_changed {
            self.focus_chunk = Some(focus_chunk);
            self.rebuild_desired(focus_chunk, world);
        }

        let unloaded =
            if focus_changed { self.unload_outside_horizon(world, focus_chunk) } else { 0 };
        let (generated, failed) = self.collect_finished(world, focus_chunk);

        if !self.candidates.is_empty() && self.priority_frustum != Some(frustum) {
            for candidate in &mut self.candidates {
                candidate.visible = chunk_intersects_frustum(candidate.position, frustum);
                candidate.priority =
                    candidate_priority(candidate.distance_squared, candidate.visible);
            }
            self.candidates.sort_by(candidate_order);
            self.priority_frustum = Some(frustum);
        }

        let mut submitted = 0;
        let mut rejected_jobs = 0;
        if !self.candidates.is_empty() {
            for candidate in &self.candidates {
                if submitted >= self.config.max_submissions_per_update {
                    break;
                }
                let position = candidate.position;
                if world.chunk(position).is_some() || self.in_flight.contains_key(&position) {
                    continue;
                }
                let ids = self.block_ids;
                let Ok(handle) = scheduler.submit(candidate.priority, move |context| {
                    context.checkpoint()?;
                    let chunk = generate_flat_terrain_chunk(position, ids);
                    context.checkpoint()?;
                    Ok::<_, ApplicationTaskExecutionError>(chunk)
                }) else {
                    rejected_jobs += 1;
                    break;
                };
                self.in_flight.insert(position, handle);
                submitted += 1;
            }
            self.candidates.retain(|candidate| {
                world.chunk(candidate.position).is_none()
                    && !self.in_flight.contains_key(&candidate.position)
            });
        }

        let visible_desired_chunks = self.candidates.iter().filter(|item| item.visible).count();
        self.last_snapshot = ChunkStreamSnapshot {
            focus_chunk: Some(focus_chunk),
            desired_chunks: self.desired.len(),
            loaded_chunks: world.loaded_chunk_count(),
            generation_jobs: self.in_flight.len(),
            generated_this_update: generated,
            unloaded_this_update: unloaded,
            visible_desired_chunks,
            failed_jobs_this_update: failed,
            rejected_jobs_this_update: rejected_jobs,
        };
        self.last_snapshot
    }

    /// Request cooperative cancellation of all outstanding generation work.
    pub fn cancel_all(&mut self) {
        for handle in self.in_flight.values() {
            let _ = handle.cancel();
        }
        self.in_flight.clear();
    }

    /// Current streaming counters, most recently updated by [`Self::update`].
    #[must_use]
    pub const fn snapshot(&self) -> ChunkStreamSnapshot {
        self.last_snapshot
    }

    fn rebuild_desired(&mut self, focus: ChunkPosition, world: &VoxelWorld) {
        self.candidates.clear();
        self.desired.clear();
        self.priority_frustum = None;
        let vertical_min = focus
            .y
            .saturating_sub(self.config.vertical_load_radius)
            .max(self.config.minimum_chunk_y());
        let vertical_max = focus
            .y
            .saturating_add(self.config.vertical_load_radius)
            .min(self.config.maximum_chunk_y());
        if vertical_min > vertical_max {
            return;
        }

        let radius = i64::from(self.config.horizontal_load_radius);
        let radius_squared = radius.saturating_mul(radius);
        for y in vertical_min..=vertical_max {
            // All chunks at or above the plane are air in this world. Keep
            // them sparse until gameplay places a block there.
            if y >= 0 {
                continue;
            }
            for dz in -self.config.horizontal_load_radius..=self.config.horizontal_load_radius {
                for dx in -self.config.horizontal_load_radius..=self.config.horizontal_load_radius {
                    let horizontal_distance_squared = i64::from(dx).pow(2) + i64::from(dz).pow(2);
                    if horizontal_distance_squared > radius_squared {
                        continue;
                    }
                    let Some(x) = i32::try_from(i64::from(focus.x) + i64::from(dx)).ok() else {
                        continue;
                    };
                    let Some(z) = i32::try_from(i64::from(focus.z) + i64::from(dz)).ok() else {
                        continue;
                    };
                    let position = ChunkPosition::new(x, y, z);
                    self.desired.insert(position);
                    let dy = i64::from(y) - i64::from(focus.y);
                    if world.chunk(position).is_none() && !self.in_flight.contains_key(&position) {
                        let distance_squared = horizontal_distance_squared + dy.pow(2);
                        self.candidates.push(Candidate {
                            position,
                            priority: ApplicationTaskPriority::Background,
                            distance_squared,
                            visible: false,
                        });
                    }
                }
            }
        }
    }

    fn collect_finished(&mut self, world: &mut VoxelWorld, focus: ChunkPosition) -> (usize, usize) {
        let mut finished = Vec::new();
        for (&position, handle) in &self.in_flight {
            match handle.try_take() {
                Ok(Some(result)) => finished.push((position, Ok(result))),
                Err(error) => finished.push((position, Err(error))),
                Ok(None) => {}
            }
        }

        let mut generated = 0;
        let mut failed = 0;
        for (position, result) in finished {
            self.in_flight.remove(&position);
            match result {
                Ok(Ok(chunk)) if within_unload_horizon(self.config, focus, position) => {
                    world.insert_generated_chunk(position, chunk);
                    generated += 1;
                }
                Ok(Ok(_) | Err(ApplicationTaskExecutionError::Cancelled))
                | Err(ApplicationTaskExecutionError::Cancelled) => {}
                Ok(Err(error)) | Err(error) => {
                    let dy = i64::from(position.y) - i64::from(focus.y);
                    let dx = i64::from(position.x) - i64::from(focus.x);
                    let dz = i64::from(position.z) - i64::from(focus.z);
                    self.candidates.push(Candidate {
                        position,
                        priority: ApplicationTaskPriority::Background,
                        distance_squared: dx.pow(2) + dy.pow(2) + dz.pow(2),
                        visible: false,
                    });
                    failed += 1;
                    nord_app::warn!(
                        target: "nord::world::streaming",
                        chunk_x = position.x,
                        chunk_y = position.y,
                        chunk_z = position.z,
                        %error,
                        "Chunk generation job failed"
                    );
                }
            }
        }
        (generated, failed)
    }

    fn unload_outside_horizon(&mut self, world: &mut VoxelWorld, focus: ChunkPosition) -> usize {
        let stale_jobs = self
            .in_flight
            .keys()
            .copied()
            .filter(|position| !within_unload_horizon(self.config, focus, *position))
            .collect::<Vec<_>>();
        for position in stale_jobs {
            if let Some(handle) = self.in_flight.remove(&position) {
                let _ = handle.cancel();
            }
        }

        let stale_chunks = world
            .loaded_chunk_positions()
            .filter(|position| !within_unload_horizon(self.config, focus, *position))
            .collect::<Vec<_>>();
        for position in &stale_chunks {
            let _ = world.unload_chunk(*position);
        }
        stale_chunks.len()
    }
}

fn within_unload_horizon(
    config: ChunkStreamConfig,
    focus: ChunkPosition,
    position: ChunkPosition,
) -> bool {
    let dx = i64::from(position.x) - i64::from(focus.x);
    let dz = i64::from(position.z) - i64::from(focus.z);
    let horizontal_radius =
        i128::from(config.horizontal_load_radius.saturating_add(config.horizontal_unload_margin));
    let vertical_radius = i64::from(config.vertical_load_radius + config.vertical_unload_margin);
    i128::from(dx).pow(2) + i128::from(dz).pow(2) <= horizontal_radius.pow(2)
        && (i64::from(position.y) - i64::from(focus.y)).abs() <= vertical_radius
        && position.y >= config.minimum_chunk_y()
        && position.y <= config.maximum_chunk_y()
}

fn candidate_priority(distance_squared: i64, visible: bool) -> ApplicationTaskPriority {
    if visible && distance_squared <= 4 {
        ApplicationTaskPriority::Critical
    } else if visible {
        ApplicationTaskPriority::High
    } else if distance_squared <= 16 {
        ApplicationTaskPriority::Normal
    } else {
        ApplicationTaskPriority::Background
    }
}

fn candidate_order(left: &Candidate, right: &Candidate) -> std::cmp::Ordering {
    right
        .priority
        .cmp(&left.priority)
        .then_with(|| left.distance_squared.cmp(&right.distance_squared))
        .then_with(|| left.position.y.cmp(&right.position.y))
        .then_with(|| left.position.x.cmp(&right.position.x))
        .then_with(|| left.position.z.cmp(&right.position.z))
}

fn chunk_intersects_frustum(position: ChunkPosition, frustum: Frustum3) -> bool {
    let Some(bounds) = chunk_bounds(position) else {
        return true;
    };
    frustum.intersects_aabb(bounds)
}

fn chunk_bounds(position: ChunkPosition) -> Option<Aabb3> {
    // Frustum and camera math use f32. Invalid bounds fail open to avoid
    // incorrectly deprioritizing chunks near coordinate precision limits.
    #[allow(clippy::cast_precision_loss)]
    let minimum = Vec3::new(
        position.x as f32 * CHUNK_SIZE as f32,
        position.y as f32 * CHUNK_SIZE as f32,
        position.z as f32 * CHUNK_SIZE as f32,
    );
    Aabb3::new(minimum, minimum + Vec3::splat(16.0)).ok()
}

#[cfg(test)]
mod tests {
    use super::{ChunkStreamConfig, candidate_order, candidate_priority};
    use nord_app::{
        ApplicationFrustum3, ApplicationPlane3, ApplicationSchedulerConfig,
        ApplicationShutdownMode, ApplicationTaskPriority, ApplicationTaskScheduler,
        ApplicationThreadPool, ApplicationThreadPoolConfig, ApplicationVec3,
    };
    use nord_world::{BlockPosition, ChunkPosition};
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn frustum_visibility_and_distance_choose_streaming_priority() {
        assert_eq!(candidate_priority(1, true), ApplicationTaskPriority::Critical);
        assert_eq!(candidate_priority(8, true), ApplicationTaskPriority::High);
        assert_eq!(candidate_priority(4, false), ApplicationTaskPriority::Normal);
        assert_eq!(candidate_priority(25, false), ApplicationTaskPriority::Background);
    }

    #[test]
    fn candidate_order_is_priority_then_near_then_stable_coordinates() {
        let mut candidates = [
            super::Candidate {
                position: ChunkPosition::new(2, -1, 0),
                priority: ApplicationTaskPriority::Normal,
                distance_squared: 4,
                visible: false,
            },
            super::Candidate {
                position: ChunkPosition::new(1, -1, 0),
                priority: ApplicationTaskPriority::High,
                distance_squared: 9,
                visible: true,
            },
            super::Candidate {
                position: ChunkPosition::new(1, -1, 0),
                priority: ApplicationTaskPriority::Normal,
                distance_squared: 1,
                visible: false,
            },
        ];
        candidates.sort_by(candidate_order);
        assert_eq!(candidates[0].priority, ApplicationTaskPriority::High);
        assert_eq!(candidates[1].distance_squared, 1);
    }

    #[test]
    fn vertical_config_counts_chunks_from_origin_in_both_directions() {
        let config = ChunkStreamConfig::new(3, 5);
        assert_eq!(config.minimum_chunk_y(), -5);
        assert_eq!(config.maximum_chunk_y(), 2);
    }

    #[test]
    fn async_streaming_generates_negative_chunks_and_unloads_outside_hysteresis() {
        let pool = Arc::new(
            ApplicationThreadPool::new(
                ApplicationThreadPoolConfig::default()
                    .with_worker_threads(NonZeroUsize::new(1).expect("one worker")),
            )
            .expect("worker pool initializes"),
        );
        let scheduler = ApplicationTaskScheduler::new(
            pool,
            ApplicationSchedulerConfig::default()
                .with_queue_capacity(NonZeroUsize::new(8).expect("nonzero queue"))
                .with_max_in_flight(NonZeroUsize::new(1)),
        )
        .expect("task scheduler initializes");
        let mut world = super::super::VoxelWorld::new(
            super::super::BlockRegistry::built_in().expect("built-in registry loads"),
        );
        let config = ChunkStreamConfig::new(2, 8)
            .with_horizontal_load_radius(0)
            .with_vertical_load_radius(0)
            .with_max_submissions_per_update(1);
        let mut streamer = super::ChunkStreamer::new(config, &world);
        let plane = |normal: ApplicationVec3, distance| {
            ApplicationPlane3::new(normal, distance).expect("valid test plane")
        };
        let frustum = ApplicationFrustum3::from_planes([
            plane(ApplicationVec3::X, 10_000.0),
            plane(-ApplicationVec3::X, 10_000.0),
            plane(ApplicationVec3::Y, 10_000.0),
            plane(-ApplicationVec3::Y, 10_000.0),
            plane(ApplicationVec3::Z, 10_000.0),
            plane(-ApplicationVec3::Z, 10_000.0),
        ]);
        let first_focus = BlockPosition::new(-1, -1, -1);
        let mut first_snapshot = streamer.update(&mut world, &scheduler, first_focus, frustum);
        for _ in 0..100 {
            if world.loaded_chunk_count() == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
            first_snapshot = streamer.update(&mut world, &scheduler, first_focus, frustum);
        }
        assert_eq!(first_snapshot.loaded_chunks, 1);
        assert_eq!(
            world.block_at(first_focus),
            world.registry().get("nord:grass").unwrap().numeric_id()
        );

        let distant_focus = BlockPosition::new(16 * 32, -1, -1);
        let mut distant_snapshot = streamer.update(&mut world, &scheduler, distant_focus, frustum);
        for _ in 0..100 {
            if distant_snapshot.loaded_chunks == 1 && distant_snapshot.generated_this_update == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
            distant_snapshot = streamer.update(&mut world, &scheduler, distant_focus, frustum);
        }
        assert_eq!(distant_snapshot.loaded_chunks, 1);
        assert_eq!(world.loaded_chunk_positions().next(), Some(distant_focus.chunk()));
        streamer.cancel_all();
        scheduler.shutdown(ApplicationShutdownMode::CancelPending).expect("scheduler shuts down");
    }
}
