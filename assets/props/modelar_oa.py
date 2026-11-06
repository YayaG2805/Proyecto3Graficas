# MODELOS DE OA (Linterna Verde) para el Proyecto 3 (Diego).
#
# Se ejecuta con Blender (no necesita abrir la ventana):
#     blender --background --python assets/props/modelar_oa.py
#
# Arma dos modelos low-poly y los exporta a assets/props/ con su .blend y sus LODs:
#   - bateria_oa: la Batería Central de Poder, una linterna gigante (pedestal escalonado,
#     núcleo que brilla, jaula de barras, tapa y asa);
#   - torre_oa: torre de la Ciudadela de los Guardianes (aguja con plataforma, anillos de
#     energía y punta brillante).
# Reutiliza la clase Model de assets/heroes/modelar_heroes.py (colores por PALETA: cada parte
# lleva una banda de u y el shader del juego, en src/oa.rs, elige el color por banda).
#
# Ejes: aquí "arriba" es +Z de Blender, que en el .obj queda como +Y (lo que espera props.rs).

import math
import os
import sys

import bpy

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "heroes"))
import modelar_heroes as mh  # noqa: E402

# Los modelos de Oa se guardan junto a los demás objetos de superficie.
mh.OUT = os.path.dirname(os.path.abspath(__file__))

# Bandas de la paleta (mismo orden que en src/oa.rs).
METAL, GLOW, TRIM = 0, 1, 2


def torus(m, band, loc, major, minor, rot=(0, 0, 0), segments=16):
    """Anillo (dona) con la banda dada."""
    bpy.ops.mesh.primitive_torus_add(
        major_radius=major, minor_radius=minor, major_segments=segments, minor_segments=6,
        location=loc, rotation=mh.rad(rot),
    )
    return m._part(band)


def bateria_oa():
    """Batería Central de Poder: ~3.3 de alto, ancho ~2.6."""
    m = mh.Model("bateria_oa", [(0.06, 0.10, 0.08), (0.15, 1.0, 0.35), (0.55, 0.95, 0.55)])
    # Pedestal escalonado de 8 lados.
    m.disc(METAL, (0, 0, 0.10), 1.30, 0.20, vertices=8)
    m.disc(METAL, (0, 0, 0.30), 1.10, 0.20, vertices=8)
    # Aro de abajo, núcleo brillante y aro de arriba.
    m.disc(TRIM, (0, 0, 0.55), 0.95, 0.25, vertices=16)
    m.disc(GLOW, (0, 0, 1.35), 0.72, 1.40, vertices=16)
    m.disc(TRIM, (0, 0, 2.15), 0.95, 0.25, vertices=16)
    # Jaula: 8 barras verticales alrededor del núcleo y dos aros finos.
    for k in range(8):
        a = k / 8 * math.tau
        m.box(METAL, (math.cos(a) * 0.84, math.sin(a) * 0.84, 1.35), (0.12, 0.12, 1.40), rot=(0, 0, math.degrees(a)))
    for z in (0.95, 1.75):
        torus(m, TRIM, (0, 0, z), 0.84, 0.05)
    # Tapa (cono truncado), botón de arriba y asa.
    m.cone(METAL, (0, 0, 2.50), 0.90, 0.35, 0.45, vertices=16)
    m.disc(GLOW, (0, 0, 2.78), 0.25, 0.12, vertices=12)
    torus(m, TRIM, (0, 0, 3.05), 0.42, 0.07, rot=(90, 0, 0))
    m.save()


def torre_oa():
    """Torre de los Guardianes: aguja de ~4.8 de alto."""
    m = mh.Model("torre_oa", [(0.08, 0.13, 0.11), (0.20, 1.0, 0.40), (0.80, 1.0, 0.80)])
    m.cone(METAL, (0, 0, 0.30), 0.70, 0.48, 0.60, vertices=6)
    m.taper(METAL, (0, 0, 0.6), (0, 0, 3.9), 0.45, 0.16, vertices=6)
    # Plataforma de los Guardianes a media altura.
    m.disc(METAL, (0, 0, 2.0), 0.85, 0.10, vertices=12)
    # Anillos de energía que rodean la aguja, cada vez más chicos.
    for z, r in ((1.30, 0.55), (2.45, 0.42), (3.30, 0.30)):
        m.disc(GLOW, (0, 0, z), r, 0.07, vertices=12)
    # Punta brillante y un anillo flotando a su alrededor.
    m.cone(TRIM, (0, 0, 4.35), 0.18, 0.0, 0.90, vertices=6)
    torus(m, GLOW, (0, 0, 4.00), 0.34, 0.04)
    m.save()


for build in (bateria_oa, torre_oa):
    build()
mh.log("LISTO OA")
