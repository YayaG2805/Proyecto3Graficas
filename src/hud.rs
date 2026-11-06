//! Partes del HUD que se dibujan en NUESTRO framebuffer (no con raylib): el radar y el
//! fondo de la ficha del planeta. raylib solo escribe el texto encima.
//!
//! Todo trabaja en píxeles de la resolución interna (640x360) y mezcla con alfa:
//! color_final = alfa * color + (1 - alfa) * color_de_abajo.

use crate::math::Vec3;

/// Radio del radar en píxeles internos.
pub const RADAR_RADIUS: f32 = 52.0;
/// Separación del radar con las orillas de la pantalla (píxeles internos).
pub const RADAR_MARGIN: f32 = 10.0;
/// Cuántas unidades del mundo caben del centro del radar a su borde.
pub const RADAR_RANGE: f32 = 220.0;
/// Opacidad del fondo del radar y de la ficha.
const PANEL_ALPHA: f32 = 0.55;
const PANEL_COLOR: Vec3 = Vec3::new(0.02, 0.03, 0.07);
const LINE_COLOR: Vec3 = Vec3::new(0.35, 0.55, 0.85);
const SHIP_COLOR: Vec3 = Vec3::new(1.0, 1.0, 1.0);

/// Un lienzo sobre el framebuffer RGBA8 del renderer.
pub struct Canvas<'a> {
    pub px: &'a mut [u8],
    pub w: usize,
    pub h: usize,
}

impl Canvas<'_> {
    /// Mezcla un color en el píxel (x, y) con opacidad `a`. Fuera de la pantalla no hace nada.
    fn blend(&mut self, x: i32, y: i32, c: Vec3, a: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || a <= 0.0 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        let a = a.min(1.0);
        for (k, v) in [c.x, c.y, c.z].into_iter().enumerate() {
            let old = self.px[i + k] as f32 / 255.0;
            self.px[i + k] = ((v.clamp(0.0, 1.0) * a + old * (1.0 - a)) * 255.0) as u8;
        }
    }

    /// Rectángulo relleno, de (x0, y0) a (x1, y1).
    pub fn fill_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Vec3, a: f32) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.blend(x, y, c, a);
            }
        }
    }

    /// Solo el borde de un rectángulo (1 píxel).
    pub fn rect_outline(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Vec3, a: f32) {
        for x in x0..x1 {
            self.blend(x, y0, c, a);
            self.blend(x, y1 - 1, c, a);
        }
        for y in y0..y1 {
            self.blend(x0, y, c, a);
            self.blend(x1 - 1, y, c, a);
        }
    }

    /// Círculo relleno con orilla suave (antialias de 1 píxel).
    pub fn fill_disc(&mut self, cx: f32, cy: f32, r: f32, c: Vec3, a: f32) {
        let (x0, x1) = ((cx - r - 1.0).floor() as i32, (cx + r + 1.0).ceil() as i32);
        let (y0, y1) = ((cy - r - 1.0).floor() as i32, (cy + r + 1.0).ceil() as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                // 1 dentro, 0 fuera, y una rampa de 1 píxel en la orilla.
                let cover = (r + 0.5 - d).clamp(0.0, 1.0);
                self.blend(x, y, c, a * cover);
            }
        }
    }

    /// Circunferencia (solo el anillo), de grosor `thick` píxeles.
    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, thick: f32, c: Vec3, a: f32) {
        let outer = r + thick * 0.5;
        let (x0, x1) = ((cx - outer - 1.0).floor() as i32, (cx + outer + 1.0).ceil() as i32);
        let (y0, y1) = ((cy - outer - 1.0).floor() as i32, (cy + outer + 1.0).ceil() as i32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let cover = (thick * 0.5 + 0.5 - (d - r).abs()).clamp(0.0, 1.0);
                self.blend(x, y, c, a * cover);
            }
        }
    }

    /// Línea recta con DDA (un paso por píxel, igual que las órbitas del renderer).
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, c: Vec3, a: f32) {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).ceil().max(1.0) as i32;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.blend((x0 + (x1 - x0) * t) as i32, (y0 + (y1 - y0) * t) as i32, c, a);
        }
    }
}

/// Un punto del radar: un cuerpo visto desde la nave.
pub struct RadarBlip {
    /// Posición del cuerpo menos la posición de la nave (en el mundo).
    pub offset: Vec3,
    /// Tamaño del punto en píxeles.
    pub size: f32,
    pub color: Vec3,
    /// Marcar con un anillo (el destino del warp).
    pub highlight: bool,
}

/// Centro del radar en píxeles internos: abajo a la derecha.
pub fn radar_center(w: usize, h: usize) -> (f32, f32) {
    (w as f32 - RADAR_MARGIN - RADAR_RADIUS, h as f32 - RADAR_MARGIN - RADAR_RADIUS)
}

/// Radar visto desde arriba: la nave en el centro mirando hacia arriba del radar.
/// `forward` = hacia dónde apunta la nave; solo importa su parte horizontal (x, z).
pub fn draw_radar(cv: &mut Canvas, forward: Vec3, blips: &[RadarBlip]) {
    let (cx, cy) = radar_center(cv.w, cv.h);

    // Fondo, anillos de distancia y cruz.
    cv.fill_disc(cx, cy, RADAR_RADIUS, PANEL_COLOR, PANEL_ALPHA);
    cv.ring(cx, cy, RADAR_RADIUS, 1.0, LINE_COLOR, 0.9);
    cv.ring(cx, cy, RADAR_RADIUS * 0.5, 1.0, LINE_COLOR, 0.35);
    cv.line(cx - RADAR_RADIUS, cy, cx + RADAR_RADIUS, cy, LINE_COLOR, 0.25);
    cv.line(cx, cy - RADAR_RADIUS, cx, cy + RADAR_RADIUS, LINE_COLOR, 0.25);

    // Base horizontal de la nave: "adelante" (f) y "derecha" (r) en el plano XZ.
    // Si la nave mira casi recto arriba o abajo, usamos -Z para no dividir entre cero.
    let flat = Vec3::new(forward.x, 0.0, forward.z);
    let f = if flat.length() > 1e-3 { flat.normalized() } else { Vec3::new(0.0, 0.0, -1.0) };
    let r = Vec3::new(-f.z, 0.0, f.x);

    let scale = RADAR_RADIUS / RADAR_RANGE;
    for b in blips {
        // Coordenadas en el marco de la nave: cuánto está a la derecha y cuánto adelante.
        let (mut x, mut y) = (b.offset.dot(r) * scale, -b.offset.dot(f) * scale);
        // Lo que queda fuera del alcance se pega a la orilla, más chico.
        let d = (x * x + y * y).sqrt();
        let mut size = b.size;
        if d > RADAR_RADIUS - 3.0 {
            x *= (RADAR_RADIUS - 3.0) / d;
            y *= (RADAR_RADIUS - 3.0) / d;
            size = size.min(1.5);
        }
        cv.fill_disc(cx + x, cy + y, size, b.color, 1.0);
        if b.highlight {
            cv.ring(cx + x, cy + y, size + 3.0, 1.0, Vec3::new(1.0, 0.85, 0.3), 1.0);
        }
    }

    // La nave: un triángulo pequeño apuntando hacia arriba del radar.
    for i in 0..5 {
        let half = i as f32 * 0.8;
        let y = cy - 3.0 + i as f32;
        cv.line(cx - half, y, cx + half, y, SHIP_COLOR, 1.0);
    }
}

/// Fondo de un panel de texto (la ficha): rectángulo oscuro con borde.
pub fn draw_panel(cv: &mut Canvas, x0: i32, y0: i32, x1: i32, y1: i32) {
    cv.fill_rect(x0, y0, x1, y1, PANEL_COLOR, PANEL_ALPHA);
    cv.rect_outline(x0, y0, x1, y1, LINE_COLOR, 0.9);
}

/// La fuente por defecto de raylib solo trae letras ASCII: quitamos acentos y eñes
/// para que no salgan como "?" en el HUD.
pub fn to_ascii(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            'á' | 'à' | 'ä' => Some('a'),
            'é' | 'è' | 'ë' => Some('e'),
            'í' | 'ì' | 'ï' => Some('i'),
            'ó' | 'ò' | 'ö' => Some('o'),
            'ú' | 'ù' | 'ü' => Some('u'),
            'Á' => Some('A'),
            'É' => Some('E'),
            'Í' => Some('I'),
            'Ó' => Some('O'),
            'Ú' => Some('U'),
            'ñ' => Some('n'),
            'Ñ' => Some('N'),
            '¿' | '¡' => None,
            c if c.is_ascii() => Some(c),
            _ => Some('?'),
        })
        .collect()
}

/// Parte un texto en renglones de máximo `max_chars` letras, sin cortar palabras.
pub fn wrap(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > max_chars {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_no_corta_palabras() {
        let lines = wrap("Su sol rojo no le daba poderes a Kal-El", 12);
        assert!(lines.iter().all(|l| l.chars().count() <= 12));
        assert_eq!(lines.join(" "), "Su sol rojo no le daba poderes a Kal-El");
    }

    #[test]
    fn el_radar_pone_adelante_arriba() {
        // Un cuerpo justo enfrente de la nave debe quedar ARRIBA del centro del radar.
        let mut px = vec![0u8; 200 * 200 * 4];
        let mut cv = Canvas { px: &mut px, w: 200, h: 200 };
        let forward = Vec3::new(0.0, 0.0, -1.0);
        let blip = RadarBlip { offset: forward * 100.0, size: 2.0, color: Vec3::new(1.0, 0.0, 0.0), highlight: false };
        draw_radar(&mut cv, forward, &[blip]);
        let (cx, cy) = radar_center(200, 200);
        let y = cy - 100.0 * RADAR_RADIUS / RADAR_RANGE;
        let i = (y as usize * 200 + cx as usize) * 4;
        assert!(cv.px[i] > 200 && cv.px[i + 1] < 50, "el punto rojo debe estar arriba del centro");
    }
}
