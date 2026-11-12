# MODELOS DE APOKOLIPS para el Proyecto 3.
#
# Se ejecuta con Blender (no necesita abrir la ventana):
#     blender --background --python assets/props/modelar_apokolips.py
#
# Arma dos modelos low-poly con su .blend y sus LODs:
#   - assets/heroes/darkseid: el villano, un gigante de piel de piedra gris con armadura azul,
#     botas y guantes oscuros y los ojos rojos (los Rayos Omega) que brillan en el juego;
#   - assets/props/nave_apokolips: su nave de guerra, un casco angular oscuro con alas en
#     punta, ventanas rojas y motores encendidos.
# Reutiliza la clase Model de assets/heroes/modelar_heroes.py (colores por PALETA: cada parte
# lleva una banda de u y el shader del juego, en src/apokolips.rs, elige el color por banda;
# las paletas deben estar en el MISMO orden que allá).
#
# Ejes: aquí "arriba" es +Z y "adelante" es +Y de Blender; en el .obj quedan como +Y y -Z
# (lo que espera el juego).

import os
import sys

import bpy

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "heroes"))
import modelar_heroes as mh  # noqa: E402

V = mh.V


def scaled(obj, scale):
    """Cambia la escala de la parte recién creada (en sus propios ejes)."""
    obj.scale = scale
    return obj


# ------------------------------------------------------------------------------------------
# DARKSEID
# ------------------------------------------------------------------------------------------
STONE = (0.45, 0.43, 0.41)
ARMOR = (0.13, 0.17, 0.38)
DARK = (0.07, 0.07, 0.09)
EYES = (1.0, 0.12, 0.05)


def darkseid():
    mh.OUT = os.path.join(HERE, "..", "heroes")
    m = mh.Model("darkseid", [STONE, ARMOR, DARK, EYES])
    stone, armor, dark, eyes = range(4)
    # De pie, piernas abiertas y brazos un poco separados del cuerpo: pose de amenaza.
    P = mh.humanoid(
        m,
        {"body": armor, "skin": stone, "shoulder": stone, "upper_arm": stone, "elbow": stone,
         "forearm": stone, "hand": stone, "foot": dark},
        {
            "kn_r": (0.17, 0.02, 0.52), "an_r": (0.21, 0, 0.1),
            "kn_l": (-0.17, 0.02, 0.52), "an_l": (-0.21, 0, 0.1),
            "el_r": (0.38, -0.02, 1.18), "wr_r": (0.4, 0.04, 0.94),
            "el_l": (-0.38, -0.02, 1.18), "wr_l": (-0.4, 0.04, 0.94),
            "sh_r": (0.29, 0, 1.47), "sh_l": (-0.29, 0, 1.47),
        },
        chest=(0.31, 0.19, 0.24),
        hips=0.22,
    )
    mh.boots(m, dark, P, height=0.34, r=0.07)
    mh.gloves(m, dark, P, length=0.18, r=0.058)
    mh.belt(m, dark, P, width=0.23)
    # Falda de armadura y hombreras.
    m.cone(armor, P["pelvis"] + V(0, 0, -0.1), 0.3, 0.22, 0.3, vertices=10)
    for x in (1, -1):
        m.box(armor, P["sh_" + ("r" if x > 0 else "l")] + V(0.03 * x, 0, 0.04), (0.2, 0.22, 0.08), rot=(0, 20 * x, 0), bevel=0.02)
    # Cabeza de piedra cuadrada: quijada ancha, ceja marcada y surcos en la frente.
    head = P["head"]
    m.box(stone, head + V(0, 0.005, -0.02), (0.25, 0.25, 0.3), bevel=0.04)
    m.box(stone, head + V(0, 0.03, -0.12), (0.27, 0.22, 0.1), bevel=0.02)  # quijada
    m.box(dark, head + V(0, 0.125, 0.045), (0.23, 0.04, 0.035), bevel=0.01)  # ceja
    for z in (0.09, 0.115):
        m.box(dark, head + V(0, 0.126, z), (0.16, 0.012, 0.008))  # surcos
    for x in (0.055, -0.055):
        m.box(eyes, head + V(x, 0.128, 0.015), (0.05, 0.012, 0.025))  # ojos (Rayos Omega)
    m.save()


# ------------------------------------------------------------------------------------------
# NAVE DE GUERRA DE APOKOLIPS
# ------------------------------------------------------------------------------------------
HULL = (0.24, 0.22, 0.24)
TRIM = (0.11, 0.10, 0.11)
WINDOWS = (1.0, 0.22, 0.05)
ENGINES = (1.0, 0.55, 0.12)


def nave_apokolips():
    mh.OUT = HERE
    m = mh.Model("nave_apokolips", [HULL, TRIM, WINDOWS, ENGINES])
    hull, trim, windows, engines = range(4)
    # Casco central y proa en punta (pirámide aplanada que apunta hacia adelante, +Y).
    m.box(hull, (0, 0, 0), (0.9, 2.6, 0.35), bevel=0.05)
    scaled(m.cone(hull, (0, 1.95, 0), 0.62, 0.0, 1.4, rot=(-90, 0, 0), vertices=4), (1, 0.45, 1))
    # Puente de mando con una franja de ventanas.
    m.box(trim, (0, -0.45, 0.33), (0.4, 0.7, 0.45), bevel=0.04)
    m.box(windows, (0, -0.09, 0.42), (0.32, 0.04, 0.06))
    # Alas en flecha con púas en la punta.
    for s in (1, -1):
        m.box(hull, (0.95 * s, -0.55, -0.05), (1.5, 0.9, 0.08), rot=(0, 8 * s, 28 * s), bevel=0.02)
        m.cone(trim, (1.6 * s, -0.05, -0.08), 0.08, 0.0, 0.9, rot=(-90, 0, 0), vertices=4)
        m.box(windows, (0.47 * s, 0.3, 0.0), (0.03, 1.4, 0.05))  # ranuras rojas en los costados
        m.cone(trim, (0.3 * s, 0.4, -0.3), 0.07, 0.0, 0.45, rot=(180, 0, 0), vertices=4)  # púas de abajo
    # Tres motores atrás.
    for x in (0.0, 0.42, -0.42):
        m.disc(trim, (x, -1.32, 0.0), 0.2, 0.12, rot=(90, 0, 0), vertices=8)
        m.disc(engines, (x, -1.39, 0.0), 0.15, 0.04, rot=(90, 0, 0), vertices=8)
    m.save()


if __name__ == "__main__":
    darkseid()
    nave_apokolips()
    mh.log("LISTO")
