//! Game-owned CPU visibility and instance batching policy.

use std::time::Instant;

use nord_app::{
    ApplicationAabb3 as Aabb3, ApplicationFrustum3 as Frustum3, ApplicationMat4 as Mat4,
    ApplicationPlane3 as Plane3, ApplicationVec3 as Vec3, ApplicationVec4 as Vec4,
};

const DEMO_INSTANCE_COUNT: usize = 1681;
const GRID_SPACING: f32 = 2.0;
const INSTANCE_STRIDE: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct BatchKey {
    pub(super) mesh_id: u32,
    pub(super) material_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct InstanceData {
    pub(super) translation: Vec3,
    pub(super) tint: Vec3,
}

#[derive(Debug, Clone, Copy)]
struct SceneInstance {
    key: BatchKey,
    data: InstanceData,
    bounds: Option<Aabb3>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DrawRange {
    pub(super) key: BatchKey,
    pub(super) first_instance: u32,
    pub(super) instance_count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CullingMetrics {
    pub(super) total_instances: u32,
    pub(super) visible_instances: u32,
    pub(super) culled_instances: u32,
    pub(super) naive_draw_calls: u32,
    pub(super) instanced_draw_calls: u32,
    pub(super) culling_ns: u64,
    pub(super) grouping_ns: u64,
}

pub(super) struct GameScene {
    instances: Vec<SceneInstance>,
    instance_data: Vec<u8>,
}

impl GameScene {
    pub(super) fn demo_grid(local_bounds: Aabb3, model: Mat4) -> Self {
        let mut instances = Vec::with_capacity(DEMO_INSTANCE_COUNT);
        for row in -20_i8..=20_i8 {
            for column in -20_i8..=20_i8 {
                let translation = Vec3::new(
                    (f32::from(column)) * GRID_SPACING,
                    0.0,
                    (f32::from(row)) * GRID_SPACING,
                );
                let bounds = transform_bounds(local_bounds, model)
                    .and_then(|bounds| translate_bounds(bounds, translation));
                let variation_step = u8::try_from(
                    (i16::from(row).wrapping_mul(17) + i16::from(column).wrapping_mul(31))
                        .rem_euclid(19),
                )
                .unwrap_or_default();
                let variation = f32::from(variation_step);
                let brightness = 0.72 + variation / 100.0;
                instances.push(SceneInstance {
                    key: BatchKey { mesh_id: 0, material_id: 0 },
                    data: InstanceData {
                        translation,
                        tint: Vec3::new(
                            brightness,
                            0.82 + variation / 120.0,
                            0.72 + variation / 100.0,
                        ),
                    },
                    bounds,
                });
            }
        }
        instances.sort_by_key(|instance| instance.key);
        let instance_data = encode_instances(&instances);
        Self { instances, instance_data }
    }

    #[cfg(test)]
    fn from_instances(mut instances: Vec<SceneInstance>) -> Self {
        instances.sort_by_key(|instance| instance.key);
        let instance_data = encode_instances(&instances);
        Self { instances, instance_data }
    }

    pub(super) fn instance_count(&self) -> usize {
        self.instances.len()
    }

    pub(super) fn instance_bytes(&self) -> &[u8] {
        &self.instance_data
    }

    pub(super) fn update_visibility(
        &self,
        view_projection: Mat4,
        visible_mask: &mut Vec<u8>,
        draw_ranges: &mut Vec<DrawRange>,
    ) -> CullingMetrics {
        let culling_start = Instant::now();
        let frustum = extract_frustum(view_projection);
        visible_mask.resize(self.instances.len(), 1);
        let mut visible_count = 0_u32;
        for (index, instance) in self.instances.iter().enumerate() {
            let visible = match (frustum, instance.bounds) {
                (Some(frustum), Some(bounds)) => conservatively_expand(bounds)
                    .is_none_or(|expanded| frustum.intersects_aabb(expanded)),
                _ => true,
            };
            if let Some(slot) = visible_mask.get_mut(index) {
                *slot = u8::from(visible);
            }
            visible_count = visible_count.saturating_add(u32::from(visible));
        }
        let culling_ns = elapsed_ns(culling_start);

        let grouping_start = Instant::now();
        draw_ranges.clear();
        let mut start = 0_usize;
        while start < self.instances.len() {
            let Some(first) = self.instances.get(start) else { break };
            let key = first.key;
            let mut index = start;
            while index < self.instances.len() {
                let Some(instance) = self.instances.get(index) else { break };
                if instance.key != key {
                    break;
                }
                if visible_mask.get(index).copied().unwrap_or(1) != 0 {
                    let range_start = index;
                    while index < self.instances.len()
                        && self.instances.get(index).is_some_and(|next| next.key == key)
                        && visible_mask.get(index).copied().unwrap_or(1) != 0
                    {
                        index += 1;
                    }
                    if let (Ok(first_instance), Ok(instance_count)) =
                        (u32::try_from(range_start), u32::try_from(index - range_start))
                    {
                        draw_ranges.push(DrawRange { key, first_instance, instance_count });
                    }
                } else {
                    index += 1;
                }
            }
            start = index.max(start + 1);
        }
        let grouping_ns = elapsed_ns(grouping_start);
        let total_instances = u32::try_from(self.instances.len()).unwrap_or(u32::MAX);
        CullingMetrics {
            total_instances,
            visible_instances: visible_count,
            culled_instances: total_instances.saturating_sub(visible_count),
            naive_draw_calls: visible_count,
            instanced_draw_calls: u32::try_from(draw_ranges.len()).unwrap_or(u32::MAX),
            culling_ns,
            grouping_ns,
        }
    }
}

fn encode_instances(instances: &[SceneInstance]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(instances.len() * INSTANCE_STRIDE);
    for instance in instances {
        for component in
            instance.data.translation.to_array().into_iter().chain(instance.data.tint.to_array())
        {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
    }
    bytes
}

fn transform_bounds(bounds: Aabb3, transform: Mat4) -> Option<Aabb3> {
    let minimum = bounds.min();
    let maximum = bounds.max();
    let mut output_min = Vec3::splat(f32::INFINITY);
    let mut output_max = Vec3::splat(f32::NEG_INFINITY);
    for x in [minimum.x, maximum.x] {
        for y in [minimum.y, maximum.y] {
            for z in [minimum.z, maximum.z] {
                let point = transform.transform_point3(Vec3::new(x, y, z))?;
                output_min = output_min.min(point);
                output_max = output_max.max(point);
            }
        }
    }
    Aabb3::new(output_min, output_max).ok()
}

fn translate_bounds(bounds: Aabb3, translation: Vec3) -> Option<Aabb3> {
    Aabb3::new(bounds.min() + translation, bounds.max() + translation).ok()
}

fn conservatively_expand(bounds: Aabb3) -> Option<Aabb3> {
    let scale =
        bounds.center().abs().max_element().max(bounds.half_extents().max_element()).max(1.0);
    let epsilon = scale * f32::EPSILON * 32.0;
    let expanded = bounds.expanded(epsilon)?;
    (expanded.min().is_finite() && expanded.max().is_finite()).then_some(expanded)
}

fn extract_frustum(view_projection: Mat4) -> Option<Frustum3> {
    let columns = view_projection.to_cols();
    let row = [
        Vec4::new(columns[0].x, columns[1].x, columns[2].x, columns[3].x),
        Vec4::new(columns[0].y, columns[1].y, columns[2].y, columns[3].y),
        Vec4::new(columns[0].z, columns[1].z, columns[2].z, columns[3].z),
        Vec4::new(columns[0].w, columns[1].w, columns[2].w, columns[3].w),
    ];
    let coefficients = [
        row[3] + row[0],
        row[3] - row[0],
        row[3] + row[1],
        row[3] - row[1],
        row[2],
        row[3] - row[2],
    ];
    let planes = coefficients.map(|plane| Plane3::new(plane.truncate(), plane.w).ok());
    Some(Frustum3::from_planes([
        planes[0]?, planes[1]?, planes[2]?, planes[3]?, planes[4]?, planes[5]?,
    ]))
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_camera::PerspectiveCamera;

    fn camera_matrix() -> Mat4 {
        let mut camera =
            PerspectiveCamera::new(60.0_f32.to_radians(), 16.0 / 9.0, 0.1, 100.0).unwrap();
        camera.look_at(Vec3::new(0.0, 0.0, -5.0), Vec3::ZERO, Vec3::Y).unwrap();
        camera.view_projection_matrix().unwrap()
    }

    #[test]
    fn conservative_frustum_culls_outside_bounds_and_keeps_intersections() {
        let frustum = extract_frustum(camera_matrix()).unwrap();
        assert!(frustum.intersects_aabb(Aabb3::new(Vec3::splat(-0.5), Vec3::splat(0.5)).unwrap()));
        assert!(!frustum.intersects_aabb(
            Aabb3::new(Vec3::new(100.0, -1.0, -1.0), Vec3::new(101.0, 1.0, 1.0)).unwrap()
        ));
        assert!(!frustum.intersects_aabb(
            Aabb3::new(Vec3::new(-1.0, -1.0, -7.0), Vec3::new(1.0, 1.0, -6.0)).unwrap()
        ));
        assert!(frustum.intersects_aabb(
            Aabb3::new(Vec3::new(-0.5, -0.5, -4.95), Vec3::new(0.5, 0.5, -4.85)).unwrap()
        ));
        assert!(!frustum.intersects_aabb(
            Aabb3::new(Vec3::new(-1.0, -1.0, 200.0), Vec3::new(1.0, 1.0, 201.0)).unwrap()
        ));
    }

    #[test]
    fn groups_visible_ranges_by_mesh_and_material_without_reordering_instance_data() {
        let visible = Some(Aabb3::new(Vec3::splat(-0.25), Vec3::splat(0.25)).unwrap());
        let outside = Some(Aabb3::new(Vec3::splat(100.0), Vec3::splat(101.0)).unwrap());
        let instances = vec![
            SceneInstance {
                key: BatchKey { mesh_id: 1, material_id: 2 },
                data: InstanceData { translation: Vec3::ZERO, tint: Vec3::ONE },
                bounds: visible,
            },
            SceneInstance {
                key: BatchKey { mesh_id: 1, material_id: 2 },
                data: InstanceData { translation: Vec3::ONE, tint: Vec3::ONE },
                bounds: visible,
            },
            SceneInstance {
                key: BatchKey { mesh_id: 1, material_id: 2 },
                data: InstanceData { translation: Vec3::ZERO, tint: Vec3::ONE },
                bounds: outside,
            },
            SceneInstance {
                key: BatchKey { mesh_id: 2, material_id: 3 },
                data: InstanceData { translation: Vec3::ZERO, tint: Vec3::ONE },
                bounds: visible,
            },
        ];
        let scene = GameScene::from_instances(instances);
        let mut mask = Vec::new();
        let mut ranges = Vec::new();
        let metrics = scene.update_visibility(camera_matrix(), &mut mask, &mut ranges);
        assert_eq!(scene.instance_bytes().len(), scene.instance_count() * INSTANCE_STRIDE);
        assert_eq!(metrics.total_instances, 4);
        assert_eq!(metrics.visible_instances, 3);
        assert_eq!(metrics.culled_instances, 1);
        assert_eq!(
            ranges,
            [
                DrawRange {
                    key: BatchKey { mesh_id: 1, material_id: 2 },
                    first_instance: 0,
                    instance_count: 2
                },
                DrawRange {
                    key: BatchKey { mesh_id: 2, material_id: 3 },
                    first_instance: 3,
                    instance_count: 1
                },
            ]
        );
        assert!(metrics.instanced_draw_calls < metrics.naive_draw_calls);
    }

    #[test]
    fn invalid_frustum_and_unknown_bounds_fail_open() {
        let scene = GameScene::from_instances(vec![SceneInstance {
            key: BatchKey { mesh_id: 0, material_id: 0 },
            data: InstanceData { translation: Vec3::ZERO, tint: Vec3::ONE },
            bounds: None,
        }]);
        let invalid_matrix = Mat4::from_cols(
            Vec4::new(f32::NAN, 0.0, 0.0, 0.0),
            Vec4::new(0.0, 1.0, 0.0, 0.0),
            Vec4::new(0.0, 0.0, 1.0, 0.0),
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        );
        let mut mask = Vec::new();
        let mut ranges = Vec::new();
        let metrics = scene.update_visibility(invalid_matrix, &mut mask, &mut ranges);
        assert_eq!(metrics.visible_instances, 1);
        assert_eq!(metrics.culled_instances, 0);
        assert_eq!(ranges.first().map(|range| range.instance_count), Some(1));
    }

    #[test]
    fn grid_has_fixed_upload_layout_and_real_culling_reduces_draw_submissions() {
        let bounds = Aabb3::new(Vec3::splat(-0.5), Vec3::splat(0.5)).unwrap();
        let scene = GameScene::demo_grid(bounds, Mat4::IDENTITY);
        let mut mask = Vec::new();
        let mut ranges = Vec::new();
        let metrics = scene.update_visibility(camera_matrix(), &mut mask, &mut ranges);
        assert_eq!(metrics.total_instances, 1681);
        assert_eq!(scene.instance_bytes().len(), 1681 * INSTANCE_STRIDE);
        assert!(metrics.visible_instances > 0);
        assert!(metrics.culled_instances > 0);
        assert!(metrics.instanced_draw_calls < metrics.naive_draw_calls);
    }
}
