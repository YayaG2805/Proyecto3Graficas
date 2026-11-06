# Proyecto Final – Sistema Solar con Software Renderer (Rust)

Sistema solar pequeño que se explora con una nave. Todo el 3D se dibuja con **nuestro propio pipeline por software**:
raylib solo se usa para abrir la ventana, leer el teclado, mostrar el framebuffer como textura 2D y escribir el texto del HUD.

## Integrantes y contribuciones

| Integrante | Parte | Archivos principales |
|---|---|---|
| Diego Guevara | Motor / pipeline de render, culling, LOD, órbitas, mapeo de normales | `renderer.rs`, `math.rs`, `mesh.rs`, `normal_map.rs` |
| _(integrante 2)_ | Planetas, shaders, superficie | `scene.rs`, `shaders.rs`, `noise.rs` |
| _(integrante 3)_ | Nave y navegación | `ship.rs`, `assets/Halcon_Milenario.obj` / `.mtl` / `.blend` |

## Cómo ejecutar

Requisitos:
- [Rust](https://rustup.rs) (stable).
- **CMake** y **LLVM/Clang**, que el crate `raylib` necesita para compilar raylib.
  - Windows: `winget install Kitware.CMake LLVM.LLVM` y las *Visual Studio Build Tools* (C++).
  - Linux: `sudo apt install cmake clang libx11-dev libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev libgl1-mesa-dev`

```bash
cargo run --release
```

> Siempre usen `--release`: un renderer por software sin optimizaciones corre muy lento.

Para generar capturas sin abrir la ventana (útil para probar cambios):

```bash
cargo run --release -- --snapshot   # genera snap_*.ppm e imprime ms/cuadro y triángulos
```

## Controles

| Tecla | Acción |
|---|---|
| W / S | Acelerar / reversa |
| A / D | Girar izquierda / derecha |
| Flechas ↑ / ↓ | Subir / bajar la nariz |
| Shift | Turbo: empuja hacia adelante y sale la llama azul por detrás |
| Espacio | Frenar |
| C | Activar/desactivar frustum culling (para comparar contadores) |
| L | Activar/desactivar LOD (para comparar contadores) |
| O | Mostrar/ocultar las órbitas de los planetas |
| N | Activar/desactivar el mapeo de normales (para comparar) |
| Tab (mantener) | Probar el efecto visual del warp |
| H | Mostrar/ocultar ayuda |

## Cómo funciona el pipeline (`src/renderer.rs`)

1. **Culling**: la esfera envolvente de cada objeto se prueba contra el frustum de la cámara; si está fuera, el objeto no se procesa.
2. **Vértices**: `clip = Projection * View * Model * posición`.
3. **Recorte** contra el plano cercano (Sutherland–Hodgman), necesario para acercarse a los planetas sin errores.
4. **División de perspectiva**, viewport y **back-face culling**.
5. **Rasterizado** con coordenadas baricéntricas e interpolación corregida por perspectiva.
6. **Z-buffer** (se guarda `1/w`; más grande = más cerca) y G-buffer con los datos del fragmento ganador.
7. **Objetos aditivos** (`draw_mesh_additive`, la llama del turbo): se dibujan al final, hacen la prueba de profundidad pero no escriben
   en el z-buffer, y su shader se SUMA en un buffer de luz (`glow`). Así se ven como luz semitransparente.
8. **Fragment shaders** en paralelo (rayon), una vez por píxel visible (*sombreado diferido*). Los píxeles vacíos usan el shader del cielo.
   Al final se suma el buffer de luz de los objetos aditivos.
   La nebulosa del cielo (ruido caro y que no cambia) se precalcula una vez en una textura de 256x128;
   esto bajó el sombreado de ~4.5 a ~2.8 ms por cuadro.

`--snapshot` imprime el tiempo por cuadro separado en **geometría** (pasos 1–7) y **sombreado** (paso 8),
para saber qué etapa optimizar.

## Estructura

```
src/
  main.rs       ventana raylib, loop, HUD, modo --snapshot
  math.rs       Vec3, Vec4, Mat4 (traslación, rotación, escala, look_at, perspectiva)
  mesh.rs       Vertex/Mesh, carga de .obj (tobj), icosfera con subdivisión, normales
  renderer.rs   pipeline por software (ver arriba) + contadores
  shaders.rs    shaders: sol (animado), planeta rocoso, nave (Blinn-Phong), llama del turbo (aditiva), cielo de estrellas
  noise.rs      ruido de gradiente 3D + fBm con semilla
  normal_map.rs mapa de normales procedural (espacio tangente) + base TBN
  texture.rs    textura propia (u, v) con interpolación bilineal
  scene.rs      sol y planetas: órbitas, rotación, LODs
  ship.rs       nave (física simple, colisión) y cámara de persecución
assets/
  Halcon_Milenario.obj / .mtl / .blend   nave actual: Halcón Milenario low-poly (1680 triángulos)
  nave.obj / nave.mtl / nave.blend   primera nave, modelada en Blender
```

## Características del menú que solicitamos evaluar

Estado: ✅ hecho · 🟡 parcial · ⬜ pendiente

| Característica | Pts | Estado | Dónde |
|---|---|---|---|
| Requisito: pipeline propio, modelo/vista/proyección, z-buffer | – | ✅ | `renderer.rs`, `math.rs` |
| Requisito: sol + nave propia + planeta que orbita y rota (con `dt`) | – | ✅ | `scene.rs`, `ship.rs` |
| Requisito: fragment shader propio | – | ✅ | `shaders.rs` |
| Planetas distinguibles (10 c/u, máx 5) | 50 | 🟡 1 de 5 | `scene.rs` |
| Shaders con efecto distinguible (6 c/u, máx 4) | 24 | ✅ sol, roca, nave, llama del turbo, órbita, cielo | `shaders.rs` |
| Shader animado | 8 | ✅ sol | `SunShader` |
| Deformación de geometría con ruido | 10 | ✅ planeta rocoso | `scene.rs` (`displace`) |
| Subdivisión procedural de geometría antes de rasterizar | 10 | ✅ icosfera | `Mesh::icosphere` |
| Descarte de objetos fuera de la vista + contador | 7 | ✅ tecla C | `Renderer::sphere_visible` |
| LOD según distancia + contador | 8 | ✅ tecla L | `Body::pick_lod` |
| Colisión básica con planetas | 4 | ✅ | `Ship::update` |
| Aceleración/frenado/giro progresivos | 8 | 🟡 aceleración sí, giro aún no | `Ship::update` |
| Modo de control local para rodear un planeta | 8 | ⬜ | |
| Atracción gravitatoria | 10 | ⬜ | |
| Modelos sobre la superficie (5 c/u) + distribución procedural + orientación | 35 | ⬜ | |
| Mapeo de normales | 8 | ✅ tecla N, planeta rocoso | `normal_map.rs`, `RockyShader` |
| Órbitas visibles (rúbrica) | – | ✅ tecla O | `Renderer::draw_line_loop`, `OrbitShader` |
| Warping animado (rúbrica) | – | 🟡 efecto visual listo (Tab); falta el viaje | `warp_streaks` en `shaders.rs`, FOV en `main.rs` |

## Uso de IA y recursos externos

Se usó asistencia de IA (Claude) para la estructura inicial del motor. Todo el código está comentado y el equipo lo revisó y entiende.
Librerías: `raylib` (ventana/entrada/textura), `tobj` (lectura de .obj), `rayon` (paralelismo).
