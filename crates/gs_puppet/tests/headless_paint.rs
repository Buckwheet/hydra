//! The crate's draw seams consumed by a real (headless) egui frame.
//!
//! No window, no GPU: an `egui::Context` runs one frame against a fixed
//! `RawInput`, and the assertions are on the shapes the frame comes back with.
//! This is the integration face of the seams `splat::render`,
//! `draw::Posed::draw_list`, `draw::Posed::ground_shadow` and `uv::Texture`
//! exposed: each now emits `epaint` values an `egui::Painter` consumes.  The
//! `Posed` methods live in `draw` / `shadow` / `skin` exactly as in the binary,
//! but they are inherent, so no extra import is needed to call them.

use gs_puppet::camera::Camera;
use gs_puppet::math::Vec3;
use gs_puppet::model::{Puppet, Segment};
use gs_puppet::splat::{self, Splat};

/// A trunk plus one child bone: radius 0.5 and 0.1.
fn rig() -> Puppet {
    Puppet::new(
        "biped",
        vec![
            Segment::new("root", 0, Vec3::ZERO, 1.0),
            Segment::new("neck", 0, Vec3::new(0.0, 1.0, 0.0), 0.2),
        ],
    )
    .unwrap()
}

fn cam(pitch: f32) -> Camera {
    Camera::new(320.0, 240.0, 0.0, pitch, 8.0, 400.0)
}

fn input() -> egui::RawInput {
    let mut input = egui::RawInput::default();
    input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(640.0, 480.0),
    ));
    input
}

/// A 4x4 opaque-white "gaussian" sheet: the shape a real splat texture has.
fn sheet() -> gs_puppet::uv::Texture {
    gs_puppet::uv::Texture::blank(4, 4)
}

fn ball(z: f32) -> Splat {
    let sigma = 0.02;
    Splat::new(
        Vec3::new(0.0, 0.0, z),
        gs_puppet::math::Mat3::from_cols_array([
            sigma, 0.0, 0.0, 0.0, sigma, 0.0, 0.0, 0.0, sigma,
        ]),
        [1.0, 0.5, 0.0, 1.0],
    )
}

#[test]
fn a_posed_figure_and_its_splats_paint_into_one_frame() {
    let ctx = egui::Context::default();
    let camera = cam(0.0);
    let puppet = rig();
    let posed = puppet.pose();

    // The texture seam: `uv::Texture` -> `epaint::ColorImage` -> a real id.
    let handle = ctx.load_texture(
        "splat-gauss",
        sheet().color_image(),
        epaint::textures::TextureOptions::LINEAR,
    );

    // The splat seam: project, sort far to near, build one quad per splat.
    let splats = vec![ball(0.0), ball(3.0)];
    let prepared = splat::prepare(&splats, &camera, 64);
    assert_eq!(prepared.len(), 2);
    let mesh = splat::mesh(&prepared, handle.id());
    assert_eq!(mesh.vertices.len(), 8);

    // The draw-list seams: one quad per bone, and the ground hull.
    let limbs = posed.shapes(&camera, &puppet, [200, 180, 160, 255]);
    let ground = posed.shadow_shapes(&camera, [0, 0, 0, 90]);
    assert_eq!(limbs.len(), 2);
    // With no pitch the ground plane projects onto a line, so there is no hull.
    assert!(ground.is_empty());

    let out = ctx.run_ui(input(), |ui| {
        let painter = ui.painter().clone().with_clip_rect(egui::Rect::EVERYTHING);
        for s in &limbs {
            painter.add(s.clone());
        }
        painter.add(epaint::Shape::mesh(mesh.clone()));
    });

    // The frame carries exactly what was painted, in that order.
    assert_eq!(out.shapes.len(), 3, "{:?}", out.shapes);
    let first = match &out.shapes[0].shape {
        epaint::Shape::Path(p) => p,
        other => panic!("expected the trunk's polygon first, got {other:?}"),
    };
    // The trunk: a 1.0-long bone, radius 0.5, at 50 px per unit.
    assert_eq!(
        first.points,
        vec![
            epaint::pos2(345.0, 240.0),
            epaint::pos2(345.0, 190.0),
            epaint::pos2(295.0, 190.0),
            epaint::pos2(295.0, 240.0),
        ]
    );
    assert_eq!(first.fill, epaint::Color32::from_rgba_unmultiplied(200, 180, 160, 255));
    let second = match &out.shapes[1].shape {
        epaint::Shape::Path(p) => p,
        other => panic!("expected the neck's polygon second, got {other:?}"),
    };
    // The child bone: 0.2 long, radius 0.1.
    assert_eq!(
        second.points,
        vec![
            epaint::pos2(325.0, 190.0),
            epaint::pos2(325.0, 180.0),
            epaint::pos2(315.0, 180.0),
            epaint::pos2(315.0, 190.0),
        ]
    );
    let third = match &out.shapes[2].shape {
        epaint::Shape::Mesh(m) => m,
        other => panic!("expected the splat mesh last, got {other:?}"),
    };
    // The mesh kept its texture (a real, allocated id) and its geometry.
    assert_eq!(third.texture_id, handle.id());
    assert_eq!(third.vertices.len(), 8);
    assert_eq!(third.indices.len(), 12);
    assert!(third.is_valid());
    assert_eq!(third.calc_bounds(), mesh.calc_bounds());
    // Both splats project onto the screen centre; the nearer one is wider, so the
    // frame's mesh bounds are the near splat's +-3 sigma of (1.6^2 + 0.3).
    let near_half = 3.0 * (1.6f32 * 1.6 + 0.3).sqrt();
    let bounds = third.calc_bounds();
    assert!((bounds.min.x - (320.0 - near_half)).abs() < 1e-3, "{bounds:?}");
    assert!((bounds.max.y - (240.0 + near_half)).abs() < 1e-3, "{bounds:?}");
    assert!((bounds.min.y - (240.0 - near_half)).abs() < 1e-3);

    // The 4x4 sheet really was uploaded as a texture by this frame.
    let uploaded = out
        .textures_delta
        .set
        .iter()
        .find(|(id, _)| *id == handle.id())
        .map(|(_, delta)| delta);
    let uploaded = uploaded.expect("the frame uploaded the splat sheet");
    assert_eq!(uploaded.image.size(), [4, 4]);
}

#[test]
fn a_tilted_camera_paints_the_ground_shadow_too() {
    let ctx = egui::Context::default();
    let camera = cam(30.0);
    // A spread figure: the ground footprints have to differ for the hull to have
    // any area.
    let puppet = Puppet::new(
        "biped",
        vec![
            Segment::new("root", 0, Vec3::ZERO, 1.0),
            Segment::new("arm", 0, Vec3::new(-0.5, 1.0, 0.3), 0.3),
            Segment::new("leg", 0, Vec3::new(0.4, 0.8, -0.6), 0.5),
        ],
    )
    .unwrap();
    let posed = puppet.pose();
    let ground = posed.shadow_shapes(&camera, [0, 0, 0, 90]);
    assert_eq!(ground.len(), 1, "a tilted ground plane has a hull");

    let out = ctx.run_ui(input(), |ui| {
        let painter = ui.painter().clone().with_clip_rect(egui::Rect::EVERYTHING);
        for s in &ground {
            painter.add(s.clone());
        }
    });
    assert_eq!(out.shapes.len(), 1);
    let hull = match &out.shapes[0].shape {
        epaint::Shape::Path(p) => p,
        other => panic!("expected the shadow polygon, got {other:?}"),
    };
    assert!(hull.closed);
    assert!(hull.points.len() >= 3);
    assert_eq!(hull.fill.a(), 90);
    assert_eq!((hull.fill.r(), hull.fill.g(), hull.fill.b()), (0, 0, 0));
    // The hull is the ground projection of the three bone origins (all at y = 0),
    // seen from a camera pitched 30 degrees.
    let xs: Vec<f32> = hull.points.iter().map(|p| p.x).collect();
    assert!(xs.iter().all(|x| x.is_finite()));
    let bounds = epaint::Shape::Path(hull.clone()).visual_bounding_rect();
    assert!(bounds.width() < 640.0 && bounds.height() < 480.0, "{bounds:?}");
}
