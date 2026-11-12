//! SONIDO del juego. raylib solo REPRODUCE los .wav de assets/sonidos/ (salen de
//! generar_sonidos.py); qué suena, cuándo y qué tan fuerte lo decide este archivo.
//!
//!   - LOOPS (`Music`, raylib los repite solos): motor, turbo, crucero del warp y el ambiente
//!     de cada mundo. En cada cuadro se les ajusta volumen, tono y paneo. El motor sube de
//!     volumen y tono con el brillo de los motores (`Ship::engine_glow`); cada ambiente se oye
//!     al acercarse a su mundo (en radios desde la superficie, como el escaneo) y suena del
//!     lado de la pantalla donde está ese mundo.
//!   - EFECTOS (`Sound`, suenan una vez): se disparan comparando el estado del juego con el
//!     del cuadro anterior (empezó el warp, se enganchó la órbita, subió el contador de cristales...).
//!
//! Si no hay tarjeta de sonido o falta un archivo, el juego sigue igual, solo sin ese sonido.

use crate::math::Vec3;
use crate::mundos::smoothstep;
use crate::scene::{Scene, CENTRAL_CITY_SEED};
use crate::shaders::central_city_bolt_started;
use crate::ship::{ShipInput, WarpPhase};
use crate::Game;
use raylib::prelude::{Music, RaylibAudio, Sound};

/// Carpeta de los .wav.
pub const CARPETA: &str = "assets/sonidos";

// --- Loops de la nave (volumen de 0 a 1, tono 1 = normal) ---
const VOL_MOTOR_MIN: f32 = 0.12;
const VOL_MOTOR_MAX: f32 = 0.45;
const TONO_MOTOR_MIN: f32 = 0.8;
const TONO_MOTOR_MAX: f32 = 1.5;
const VOL_TURBO: f32 = 0.55;
const VOL_WARP: f32 = 0.6;
/// Alarma de calor: empieza a sonar con este calor y llega a todo volumen con el otro.
const VOL_ALARMA: f32 = 0.45;
const ALARMA_DESDE: f32 = 0.25;
const ALARMA_TODA: f32 = 0.8;

// --- Ambientes de los mundos ---
const VOL_AMBIENTE: f32 = 0.55;
/// El sol retumba mucho; un poco más bajo para que no tape lo demás.
const VOL_AMBIENTE_SOL: f32 = 0.4;
/// Distancia (en radios desde la superficie) donde el ambiente suena completo y donde ya no se oye.
const AMBIENTE_CERCA: f32 = 0.5;
const AMBIENTE_LEJOS: f32 = 3.0;
/// Qué tanto se va un ambiente a un lado: 0 = siempre al centro, 1 = un solo oído.
const PANEO: f32 = 0.7;

// --- Choque contra un planeta (el golpe es la velocidad que la colisión le quitó a la nave) ---
const CHOQUE_MIN: f32 = 3.0;
const CHOQUE_MAX: f32 = 25.0;
/// Segundos antes de que otro choque pueda sonar (si no, rozar el planeta suena como metralleta).
const CHOQUE_ESPERA: f32 = 0.5;

/// Escala pentatónica en semitonos: cada cristal recogido suena un poco más agudo que el anterior.
const ESCALA_CRISTALES: [f32; 10] = [0.0, 2.0, 4.0, 7.0, 9.0, 12.0, 14.0, 16.0, 19.0, 21.0];

/// Loops de la nave, en el orden en que se cargan.
pub const LOOPS: [&str; 4] = ["motor", "turbo", "warp_crucero", "alarma_calor"];

/// Los efectos que suenan una vez. El número de cada uno es su lugar en `Sonidos::efectos`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Efecto {
    WarpCarga,
    WarpSalto,
    WarpLlegada,
    WarpCancelar,
    OrbitaEnganchar,
    OrbitaSoltar,
    Choque,
    Escaneo,
    Cristal,
    CristalesCompletos,
    Destino,
    Foto,
    Interruptor,
    Error,
    Rayo,
    Explosion,
}

impl Efecto {
    pub const TODOS: [Efecto; 16] = [
        Efecto::WarpCarga,
        Efecto::WarpSalto,
        Efecto::WarpLlegada,
        Efecto::WarpCancelar,
        Efecto::OrbitaEnganchar,
        Efecto::OrbitaSoltar,
        Efecto::Choque,
        Efecto::Escaneo,
        Efecto::Cristal,
        Efecto::CristalesCompletos,
        Efecto::Destino,
        Efecto::Foto,
        Efecto::Interruptor,
        Efecto::Error,
        Efecto::Rayo,
        Efecto::Explosion,
    ];

    /// Nombre del archivo (sin .wav).
    pub fn archivo(self) -> &'static str {
        match self {
            Efecto::WarpCarga => "warp_carga",
            Efecto::WarpSalto => "warp_salto",
            Efecto::WarpLlegada => "warp_llegada",
            Efecto::WarpCancelar => "warp_cancelar",
            Efecto::OrbitaEnganchar => "orbita_enganchar",
            Efecto::OrbitaSoltar => "orbita_soltar",
            Efecto::Choque => "choque",
            Efecto::Escaneo => "escaneo",
            Efecto::Cristal => "cristal",
            Efecto::CristalesCompletos => "cristales_completos",
            Efecto::Destino => "destino",
            Efecto::Foto => "foto",
            Efecto::Interruptor => "interruptor",
            Efecto::Error => "error",
            Efecto::Rayo => "rayo",
            Efecto::Explosion => "explosion",
        }
    }

    /// Volumen de cada efecto (los .wav vienen todos al mismo pico; aquí se mezclan entre sí).
    fn volumen(self) -> f32 {
        match self {
            Efecto::Error => 0.35,
            Efecto::Interruptor => 0.5,
            Efecto::Destino => 0.5,
            Efecto::Foto => 0.9,
            Efecto::CristalesCompletos => 0.9,
            Efecto::Explosion => 1.0,
            _ => 0.7,
        }
    }
}

/// Archivo del ambiente de un mundo: "Central City" → "amb_central_city".
pub fn ambiente_de(nombre: &str) -> String {
    format!("amb_{}", nombre.to_lowercase().replace(' ', "_"))
}

fn ruta(nombre: &str) -> String {
    format!("{CARPETA}/{nombre}.wav")
}

/// Qué tan fuerte se oye un mundo a `radios` de su superficie: 1 encima, 0 lejos.
fn cercania(radios: f32) -> f32 {
    1.0 - smoothstep(AMBIENTE_CERCA, AMBIENTE_LEJOS, radios)
}

/// Paneo de raylib para algo en la dirección `dir` (de la cámara hacia él, de largo 1).
/// En raylib 0.5 es el centro y 1 es TODO a la izquierda, por eso va con signo menos.
fn paneo(dir: Vec3, derecha: Vec3) -> f32 {
    0.5 - 0.5 * PANEO * dir.dot(derecha).clamp(-1.0, 1.0)
}

/// Tono del cristal número `n` (1 = el primero): sube por la escala, 2^(semitonos / 12).
fn tono_cristal(n: u32) -> f32 {
    let paso = (n.max(1) as usize - 1).min(ESCALA_CRISTALES.len() - 1);
    2f32.powf(ESCALA_CRISTALES[paso] / 12.0)
}

fn mezcla(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn cargar_loop<'a>(audio: &'a RaylibAudio, nombre: &str) -> Option<Music<'a>> {
    match audio.new_music(&ruta(nombre)) {
        Ok(m) => {
            // Todos los loops suenan siempre; el volumen decide si se oyen.
            m.set_volume(0.0);
            m.play_stream();
            Some(m)
        }
        Err(e) => {
            eprintln!("No se pudo cargar {} ({e}): se juega sin ese sonido", ruta(nombre));
            None
        }
    }
}

fn cargar_efecto<'a>(audio: &'a RaylibAudio, e: Efecto) -> Option<Sound<'a>> {
    match audio.new_sound(&ruta(e.archivo())) {
        Ok(s) => {
            s.set_volume(e.volumen());
            Some(s)
        }
        Err(err) => {
            eprintln!("No se pudo cargar {} ({err}): se juega sin ese sonido", ruta(e.archivo()));
            None
        }
    }
}

/// Lo que valía el estado del juego el cuadro pasado, para notar los cambios.
struct Antes {
    fase: Option<WarpPhase>,
    orbita: Option<usize>,
    cristales: u32,
    tiempo: f32,
    explotando: bool,
}

pub struct Sonidos<'a> {
    audio: &'a RaylibAudio,
    /// Tecla M: todo en silencio (volumen maestro en 0).
    pub silencio: bool,
    motor: Option<Music<'a>>,
    turbo: Option<Music<'a>>,
    warp: Option<Music<'a>>,
    alarma: Option<Music<'a>>,
    /// (índice del cuerpo, su loop de ambiente)
    ambientes: Vec<(usize, Music<'a>)>,
    /// Un lugar por cada `Efecto`, en el orden de `Efecto::TODOS`.
    efectos: Vec<Option<Sound<'a>>>,
    /// Índice de Central City, para que sus rayos truenen.
    luna_rayos: Option<usize>,
    antes: Antes,
    espera_choque: f32,
}

impl<'a> Sonidos<'a> {
    pub fn new(audio: &'a RaylibAudio, scene: &Scene) -> Sonidos<'a> {
        let [motor, turbo, warp, alarma] = LOOPS.map(|nombre| cargar_loop(audio, nombre));
        let ambientes = scene
            .bodies
            .iter()
            .enumerate()
            .filter_map(|(i, b)| cargar_loop(audio, &ambiente_de(b.name)).map(|m| (i, m)))
            .collect();
        Sonidos {
            audio,
            silencio: false,
            motor,
            turbo,
            warp,
            alarma,
            ambientes,
            efectos: Efecto::TODOS.iter().map(|&e| cargar_efecto(audio, e)).collect(),
            luna_rayos: crate::mundos::body(scene, "Central City"),
            antes: Antes { fase: None, orbita: None, cristales: 0, tiempo: 0.0, explotando: false },
            espera_choque: 0.0,
        }
    }

    pub fn tocar(&self, e: Efecto) {
        if let Some(s) = &self.efectos[e as usize] {
            s.play();
        }
    }

    fn efecto(&self, e: Efecto) -> Option<&Sound<'a>> {
        self.efectos[e as usize].as_ref()
    }

    /// Tecla M.
    pub fn alternar_silencio(&mut self) {
        self.silencio = !self.silencio;
        self.audio.set_master_volume(if self.silencio { 0.0 } else { 1.0 });
    }

    /// Una vez por cuadro, DESPUÉS de `Game::update` y con la misma entrada.
    pub fn update(&mut self, g: &Game, input: &ShipInput, dt: f32) {
        let (ship, st) = (&g.ship, &g.state);
        // En modo foto la nave está congelada: sus loops se callan, los ambientes siguen.
        let congelado = st.photo_mode;
        // Mientras explota la nave no existe: tampoco suenan sus loops ni sus efectos.
        let explotando = g.explosion.is_some();
        // En la pantalla de inicio la nave tampoco vuela.
        let callada = congelado || explotando || g.title;

        // --- 1. Loops de la nave ---
        let brillo = ship.engine_glow();
        let motor = if callada { 0.0 } else { mezcla(VOL_MOTOR_MIN, VOL_MOTOR_MAX, brillo) };
        ajustar(self.motor.as_ref(), motor, mezcla(TONO_MOTOR_MIN, TONO_MOTOR_MAX, brillo), 0.5);
        ajustar(self.turbo.as_ref(), if callada { 0.0 } else { VOL_TURBO * ship.boost }, 1.0, 0.5);
        ajustar(self.warp.as_ref(), if callada { 0.0 } else { VOL_WARP * g.warp }, 1.0, 0.5);
        let alarma = if callada { 0.0 } else { VOL_ALARMA * smoothstep(ALARMA_DESDE, ALARMA_TODA, g.heat) };
        ajustar(self.alarma.as_ref(), alarma, 1.0, 0.5);

        // --- 2. Ambientes: más fuerte entre más cerca, y del lado donde está el mundo ---
        let (derecha, _, _) = g.camera.basis();
        let mut vol_luna = 0.0;
        for (i, m) in &self.ambientes {
            let b = &g.scene.bodies[*i];
            let centro = g.scene.body_position(*i, g.time);
            let radios = ((centro - ship.position).length() - b.radius) / b.radius;
            let base = if b.orbit_radius == 0.0 { VOL_AMBIENTE_SOL } else { VOL_AMBIENTE };
            let vol = base * cercania(radios);
            ajustar(Some(m), vol, 1.0, paneo((centro - g.camera.position).normalized(), derecha));
            if Some(*i) == self.luna_rayos {
                vol_luna = vol;
            }
        }

        // --- 3. La explosión: empieza (BUM) y termina (la nave reaparece) ---
        if explotando && !self.antes.explotando {
            if let Some(s) = self.efecto(Efecto::WarpCarga) {
                s.stop();
            }
            self.tocar(Efecto::Explosion);
        } else if !explotando && self.antes.explotando {
            self.tocar(Efecto::WarpLlegada);
        }

        // --- 4. Efectos de la nave (en modo foto no cambia nada, así que no suenan; al explotar
        // y al reaparecer la nave cambia de golpe, y esos cambios no son del jugador) ---
        let fase = ship.warp_phase();
        if !congelado && !explotando && !self.antes.explotando {
            match (self.antes.fase, fase) {
                (None, Some(_)) => self.tocar(Efecto::WarpCarga),
                (Some(WarpPhase::Align), Some(WarpPhase::Accelerate)) => self.tocar(Efecto::WarpSalto),
                (Some(WarpPhase::Brake), None) => self.tocar(Efecto::WarpLlegada),
                (Some(_), None) => {
                    if let Some(s) = self.efecto(Efecto::WarpCarga) {
                        s.stop();
                    }
                    self.tocar(Efecto::WarpCancelar);
                }
                _ => {}
            }
            // Tab que no hizo nada: el destino está demasiado cerca.
            if input.warp && self.antes.fase.is_none() && fase.is_none() {
                self.tocar(Efecto::Error);
            }

            match (self.antes.orbita, st.orbit_lock) {
                (None, Some(_)) => self.tocar(Efecto::OrbitaEnganchar),
                // al arrancar el warp también se suelta la órbita, pero ahí ya suena la carga
                (Some(_), None) if fase.is_none() => self.tocar(Efecto::OrbitaSoltar),
                _ => {}
            }
            // F que no hizo nada: lejos del planeta o viajando.
            if input.orbit_toggle && self.antes.orbita == st.orbit_lock {
                self.tocar(Efecto::Error);
            }

            if input.gravity_toggle {
                self.tocar(Efecto::Interruptor);
            }

            self.espera_choque -= dt;
            if ship.impact > CHOQUE_MIN && self.espera_choque <= 0.0 {
                if let Some(s) = self.efecto(Efecto::Choque) {
                    let fuerza = ((ship.impact - CHOQUE_MIN) / (CHOQUE_MAX - CHOQUE_MIN)).clamp(0.3, 1.0);
                    s.set_volume(Efecto::Choque.volumen() * fuerza);
                    s.play();
                }
                self.espera_choque = CHOQUE_ESPERA;
            }

            // Trueno de Central City justo cuando empieza un rayo del shader (mismo reloj).
            if vol_luna > 0.01 && central_city_bolt_started(CENTRAL_CITY_SEED, self.antes.tiempo, g.time) {
                if let Some(s) = self.efecto(Efecto::Rayo) {
                    s.set_volume((vol_luna * 1.3).min(1.0));
                    s.play();
                }
            }
        }

        // --- 5. Efectos que funcionan siempre, también en modo foto ---
        if input.scan && !explotando {
            let en_rango = st.nearest.is_some_and(|i| st.nearest_dist < crate::SCAN_HINT_RADII * g.scene.bodies[i].radius);
            self.tocar(if en_rango { Efecto::Escaneo } else { Efecto::Error });
        }
        if let Some(i) = input.select {
            self.tocar(if st.warp_target == i { Efecto::Destino } else { Efecto::Error });
        }
        if input.photo {
            self.tocar(Efecto::Foto);
        }
        if st.collected > self.antes.cristales {
            if st.collected >= st.total {
                self.tocar(Efecto::CristalesCompletos);
            } else if let Some(s) = self.efecto(Efecto::Cristal) {
                s.set_pitch(tono_cristal(st.collected));
                s.play();
            }
        }

        self.antes = Antes { fase, orbita: st.orbit_lock, cristales: st.collected, tiempo: g.time, explotando };
    }
}

/// Le pasa a un loop su volumen, tono y paneo de este cuadro. raylib necesita `update_stream`
/// en cada cuadro para seguir leyendo el archivo.
fn ajustar(m: Option<&Music>, volumen: f32, tono: f32, pan: f32) {
    if let Some(m) = m {
        m.update_stream();
        m.set_volume(volumen);
        m.set_pitch(tono);
        m.set_pan(pan);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lee el encabezado de un .wav y revisa que raylib lo pueda abrir: PCM, mono, 16 bits.
    fn wav_valido(nombre: &str) -> Result<(), String> {
        let datos = std::fs::read(ruta(nombre)).map_err(|e| format!("{nombre}: {e}"))?;
        let u16_en = |i: usize| u16::from_le_bytes([datos[i], datos[i + 1]]);
        if datos.len() < 44 || &datos[0..4] != b"RIFF" || &datos[8..12] != b"WAVE" {
            return Err(format!("{nombre}: no es un .wav"));
        }
        if u16_en(20) != 1 || u16_en(22) != 1 || u16_en(34) != 16 {
            return Err(format!("{nombre}: debe ser PCM, mono y de 16 bits"));
        }
        Ok(())
    }

    #[test]
    fn estan_todos_los_wav() {
        let scene = Scene::new();
        let ambientes = scene.bodies.iter().map(|b| ambiente_de(b.name));
        let nombres: Vec<String> = LOOPS
            .iter()
            .map(|s| s.to_string())
            .chain(Efecto::TODOS.iter().map(|e| e.archivo().to_string()))
            .chain(ambientes)
            .collect();
        assert_eq!(nombres.len(), LOOPS.len() + Efecto::TODOS.len() + scene.bodies.len());
        for n in &nombres {
            wav_valido(n).unwrap();
        }
    }

    #[test]
    fn cada_efecto_esta_en_su_lugar() {
        // `tocar` usa `e as usize` como índice, así que TODOS tiene que ir en el orden del enum
        for (k, e) in Efecto::TODOS.iter().enumerate() {
            assert_eq!(*e as usize, k);
        }
    }

    #[test]
    fn el_ambiente_se_oye_cerca_y_se_apaga_lejos() {
        assert_eq!(cercania(0.0), 1.0);
        assert_eq!(cercania(AMBIENTE_LEJOS + 1.0), 0.0);
        let mut antes = 1.0;
        for k in 0..40 {
            let ahora = cercania(k as f32 * 0.1);
            assert!(ahora <= antes);
            antes = ahora;
        }
    }

    #[test]
    fn el_paneo_pone_cada_mundo_de_su_lado() {
        let derecha = Vec3::new(1.0, 0.0, 0.0);
        // en raylib el paneo alto es la izquierda
        assert!(paneo(derecha, derecha) < 0.5);
        assert!(paneo(-derecha, derecha) > 0.5);
        assert_eq!(paneo(Vec3::new(0.0, 0.0, -1.0), derecha), 0.5);
    }

    #[test]
    fn cada_cristal_suena_mas_agudo() {
        assert_eq!(tono_cristal(1), 1.0);
        for n in 2..=10 {
            assert!(tono_cristal(n) > tono_cristal(n - 1));
        }
    }

    #[test]
    fn el_trueno_va_con_los_rayos() {
        // cada rayo vuelve a empezar cada 3 s, así que en 3 s al menos uno empieza
        assert!(central_city_bolt_started(CENTRAL_CITY_SEED, 10.0, 13.0));
        // sin que pase el tiempo (modo foto) no empieza ninguno
        assert!(!central_city_bolt_started(CENTRAL_CITY_SEED, 10.0, 10.0));
    }
}
