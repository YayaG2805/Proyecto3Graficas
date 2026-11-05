//! Proyecto final Gráficas por Computadora — Sistema solar con renderer por software.
//!
//! raylib SOLO se usa para la ventana, leer teclado/mouse, mostrar nuestro framebuffer
//! como textura 2D y escribir texto del HUD. Todo el 3D lo hace `renderer.rs`.

mod math;
mod mesh;
mod noise;
mod normal_map;
mod renderer;
mod scene;
mod shaders;
mod ship;

use math::{Mat4, Vec3};
use mesh::Mesh;
use raylib::prelude::*;
use renderer::{Renderer, Uniforms};
use scene::Scene;
use shaders::{sky, ShipShader};
use ship::{ChaseCamera, Ship, ShipInput};
use std::sync::Arc;

/// Resolución INTERNA del renderer (se escala a la ventana). Bajarla = más FPS.
const RENDER_W: usize = 640;
const RENDER_H: usize = 360;
const WINDOW_W: i32 = 1280;
const WINDOW_H: i32 = 720;

const FOV_Y: f32 = 60.0 * std::f32::consts::PI / 180.0;
const NEAR: f32 = 0.1;
const FAR: f32 = 2000.0;

/// Todo lo que necesitamos para dibujar un cuadro.
struct Game {
    renderer: Renderer,
    scene: Scene,
    ship: Ship,
    ship_mesh: Mesh,
    ship_shader: Arc<ShipShader>,
    camera: ChaseCamera,
    time: f32,
    /// Mapeo de normales encendido (tecla N).
    normal_mapping: bool,
}

impl Game {
    fn new() -> Game {
        let ship_mesh = Mesh::load_obj("assets/nave.obj", Some(1.5)).unwrap_or_else(|e| {
            eprintln!("No se pudo cargar la nave ({e}); usando nave provisional");
            Mesh::placeholder_ship()
        });
        let ship = Ship::new(Vec3::new(0.0, 3.0, 70.0));
        let camera = ChaseCamera::new(&ship);
        let uniforms = Game::make_uniforms(&camera, 0.0, true);
        Game {
            renderer: Renderer::new(RENDER_W, RENDER_H, uniforms),
            scene: Scene::new(),
            ship,
            ship_mesh,
            ship_shader: Arc::new(ShipShader { base: Vec3::new(0.15, 0.45, 0.85) }), // Kd del material "Casco"
            camera,
            time: 0.0,
            normal_mapping: true,
        }
    }

    fn make_uniforms(camera: &ChaseCamera, time: f32, normal_mapping: bool) -> Uniforms {
        let (r, u, f) = camera.basis();
        let aspect = RENDER_W as f32 / RENDER_H as f32;
        Uniforms {
            time,
            view: camera.view(),
            projection: Mat4::perspective(FOV_Y, aspect, NEAR, FAR),
            camera_pos: camera.position,
            camera_right: r,
            camera_up: u,
            camera_forward: f,
            fov_y: FOV_Y,
            aspect,
            near: NEAR,
            far: FAR,
            light_pos: Vec3::ZERO, // el sol está en el origen
            normal_mapping,
        }
    }

    fn update(&mut self, input: ShipInput, dt: f32) {
        self.time += dt;
        self.ship.update(input, dt, &self.scene, self.time);
        self.camera.update(&self.ship, dt);
    }

    fn render(&mut self) {
        let u = Game::make_uniforms(&self.camera, self.time, self.normal_mapping);
        self.renderer.begin_frame(u);
        self.scene.draw(&mut self.renderer, self.time);
        let model = self.ship.model_matrix(1.0);
        self.renderer.draw_mesh(&self.ship_mesh, &model, self.ship_shader.clone());
        self.renderer.finish_frame(&sky);
    }
}

/// Modo sin ventana: `cargo run --release -- --snapshot` guarda capturas PPM.
/// Sirve para probar el renderer sin abrir la ventana.
fn snapshot() {
    let mut g = Game::new();
    // (archivo, posición de la nave, mapeo de normales)
    let shots = [
        ("snap_lejos.ppm", Vec3::new(0.0, 25.0, 95.0), true),
        ("snap_cerca.ppm", Vec3::new(45.0, 4.0, 14.0), true),
        ("snap_superficie.ppm", Vec3::new(46.5, 4.6, 2.5), true),
        // Misma vista sin mapeo de normales, para comparar.
        ("snap_superficie_sin_normales.ppm", Vec3::new(46.5, 4.6, 2.5), false),
    ];
    for (name, pos, normal_mapping) in shots {
        g.normal_mapping = normal_mapping;
        g.ship.position = pos;
        // Apuntar la nave hacia el planeta rocoso.
        let target = g.scene.bodies[1].position(g.time);
        let d = (target - pos).normalized();
        g.ship.yaw = (-d.x).atan2(-d.z);
        g.ship.pitch = d.y.asin();
        g.camera = ChaseCamera::new(&g.ship);
        // Medir rendimiento: promedio de 30 cuadros.
        let start = std::time::Instant::now();
        for _ in 0..30 {
            g.render();
        }
        let ms = start.elapsed().as_secs_f32() * 1000.0 / 30.0;
        g.renderer.save_ppm(name).expect("no se pudo guardar");
        let s = g.renderer.stats;
        println!(
            "{name}: {ms:.1} ms/cuadro  dibujados={} descartados={} tri_in={} tri_raster={}",
            s.objects_drawn, s.objects_culled, s.triangles_in, s.triangles_rasterized
        );
    }
}

fn read_input(rl: &RaylibHandle) -> ShipInput {
    let axis = |pos: KeyboardKey, neg: KeyboardKey| (rl.is_key_down(pos) as i32 - rl.is_key_down(neg) as i32) as f32;
    ShipInput {
        thrust: axis(KeyboardKey::KEY_W, KeyboardKey::KEY_S),
        turn: axis(KeyboardKey::KEY_D, KeyboardKey::KEY_A),
        climb: axis(KeyboardKey::KEY_UP, KeyboardKey::KEY_DOWN),
        boost: rl.is_key_down(KeyboardKey::KEY_LEFT_SHIFT),
        brake: rl.is_key_down(KeyboardKey::KEY_SPACE),
    }
}

fn main() {
    if std::env::args().any(|a| a == "--snapshot") {
        snapshot();
        return;
    }

    let (mut rl, thread) = raylib::init().size(WINDOW_W, WINDOW_H).title("Sistema Solar - Software Renderer").build();
    rl.set_target_fps(60);

    let mut game = Game::new();

    // Textura donde se copia nuestro framebuffer cada cuadro.
    let image = Image::gen_image_color(RENDER_W as i32, RENDER_H as i32, Color::BLACK);
    let mut texture = rl.load_texture_from_image(&thread, &image).expect("no se pudo crear la textura");
    let mut show_help = true;

    while !rl.window_should_close() {
        // dt limitado para que un tirón (p.ej. al mover la ventana) no teletransporte la nave.
        let dt = rl.get_frame_time().min(0.05);

        if rl.is_key_pressed(KeyboardKey::KEY_C) {
            game.renderer.frustum_culling = !game.renderer.frustum_culling;
        }
        if rl.is_key_pressed(KeyboardKey::KEY_L) {
            game.scene.use_lod = !game.scene.use_lod;
        }
        if rl.is_key_pressed(KeyboardKey::KEY_O) {
            game.scene.show_orbits = !game.scene.show_orbits;
        }
        if rl.is_key_pressed(KeyboardKey::KEY_N) {
            game.normal_mapping = !game.normal_mapping;
        }
        if rl.is_key_pressed(KeyboardKey::KEY_H) {
            show_help = !show_help;
        }

        let input = read_input(&rl);
        game.update(input, dt);
        game.render();
        texture.update_texture(&game.renderer.color).expect("no se pudo actualizar la textura");

        let fps = rl.get_fps();
        let mut d = rl.begin_drawing(&thread);
        d.clear_background(Color::BLACK);
        d.draw_texture_pro(
            &texture,
            Rectangle::new(0.0, 0.0, RENDER_W as f32, RENDER_H as f32),
            Rectangle::new(0.0, 0.0, WINDOW_W as f32, WINDOW_H as f32),
            Vector2::zero(),
            0.0,
            Color::WHITE,
        );

        // HUD
        let s = game.renderer.stats;
        let speed = game.ship.velocity.length();
        d.draw_text(&format!("FPS: {fps}   velocidad: {speed:.1}"), 10, 10, 20, Color::RAYWHITE);
        d.draw_text(
            &format!(
                "objetos: {} dibujados / {} descartados   triangulos: {} entran / {} rasterizados",
                s.objects_drawn, s.objects_culled, s.triangles_in, s.triangles_rasterized
            ),
            10,
            34,
            18,
            Color::LIGHTGRAY,
        );
        d.draw_text(
            &format!(
                "culling [C]: {}   LOD [L]: {}   orbitas [O]: {}   normales [N]: {}",
                if game.renderer.frustum_culling { "ON" } else { "OFF" },
                if game.scene.use_lod { "ON" } else { "OFF" },
                if game.scene.show_orbits { "ON" } else { "OFF" },
                if game.normal_mapping { "ON" } else { "OFF" }
            ),
            10,
            56,
            18,
            Color::LIGHTGRAY,
        );
        if show_help {
            d.draw_text(
                "W/S acelerar/frenar   A/D girar   Flechas subir/bajar   Shift turbo   Espacio frenar   O orbitas   N normales   H ocultar ayuda",
                10,
                WINDOW_H - 28,
                18,
                Color::GOLD,
            );
        }
    }
}
