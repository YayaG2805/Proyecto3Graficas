# MONUMENTOS Y DETALLES DE CADA MUNDO para el Proyecto 3 (Diego).
#
# Se ejecuta con Blender (no necesita abrir la ventana):
#     blender --background --python assets/props/modelar_mundos.py
#
# Arma modelos low-poly y los exporta a assets/props/ con su .blend y sus LODs:
#   templo          Themyscira: templo griego (escalones, columnas, frontón)
#   fortaleza       Krypton: la Fortaleza de la Soledad (racimo de agujas de cristal)
#   torre_wayne     Gotham: rascacielos escalonado con la "W" en lo alto
#   dirigible       Gotham: dirigible con góndola, aletas y pantallas de anuncios
#   ciudad_atlantis Atlantis: ciudad de torres y cúpulas (la cúpula de cristal se hace en el juego)
#   pez             Atlantis: pez para los cardúmenes
#   star_labs       Central City: S.T.A.R. Labs (edificio redondo con domo)
#   puno_oa         Oa: puño gigante, el "constructo" de luz de Linterna Verde
# Reutiliza la clase Model de assets/heroes/modelar_heroes.py: cada parte lleva una banda de u
# (paleta) y el shader del juego (src/mundos/) elige el color por banda.
#
# Ejes: "arriba" es +Z de Blender (en el .obj queda +Y). Lo que se mueve mira hacia +Y de
# Blender, que en el .obj queda hacia -Z (hacia donde avanza en el juego).

import math
import os
import sys

import bpy

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "heroes"))
import modelar_heroes as mh  # noqa: E402

mh.OUT = os.path.dirname(os.path.abspath(__file__))


def custom(m, band, verts, faces):
    """Parte hecha a mano con sus vértices y caras (p. ej. el techo de dos aguas)."""
    mesh = bpy.data.meshes.new("parte")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new("parte", mesh)
    bpy.context.collection.objects.link(obj)
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    return m._part(band)


def templo():
    STONE, GOLD, ROOF = 0, 1, 2
    m = mh.Model("templo", [(0.92, 0.90, 0.84), (1.0, 0.78, 0.30), (0.25, 0.40, 0.75)])
    # Tres escalones (estilóbato).
    for k, (w, d) in enumerate([(3.2, 2.2), (3.0, 2.0), (2.8, 1.8)]):
        m.box(STONE, (0, 0, 0.075 + 0.15 * k), (w, d, 0.15))
    # Columnas alrededor (6 al frente y atrás, 4 a los lados) con su capitel.
    spots = {(x, y) for x in [-1.2 + 0.48 * i for i in range(6)] for y in (-0.75, 0.75)}
    spots |= {(x, y) for x in (-1.2, 1.2) for y in (-0.25, 0.25)}
    for x, y in sorted(spots):
        m.disc(STONE, (x, y, 1.15), 0.10, 1.40, vertices=8)
        m.box(GOLD, (x, y, 1.89), (0.26, 0.26, 0.08))
    # Cuarto interior (cella), friso dorado y techo de dos aguas con sus frontones.
    m.box(STONE, (0, 0, 1.15), (1.7, 0.9, 1.4))
    m.box(GOLD, (0, 0, 2.03), (2.85, 1.85, 0.2))
    custom(m, ROOF,
           [(-1.45, -0.95, 2.13), (1.45, -0.95, 2.13), (1.45, 0.95, 2.13), (-1.45, 0.95, 2.13), (-1.45, 0, 2.65), (1.45, 0, 2.65)],
           [(0, 3, 2, 1), (0, 1, 5, 4), (3, 4, 5, 2), (0, 4, 3), (1, 2, 5)])
    m.save()


def fortaleza():
    ICE, BASE, CORE = 0, 1, 2
    m = mh.Model("fortaleza", [(0.75, 0.90, 1.0), (0.30, 0.45, 0.60), (0.40, 0.95, 1.0)])
    m.disc(BASE, (0, 0, 0.1), 1.6, 0.2, vertices=10)
    # Agujas: (ángulo alrededor, distancia al centro, alto, grosor, inclinación hacia afuera).
    spires = [(0, 0.0, 4.2, 0.30, 0), (40, 0.55, 3.0, 0.22, 12), (110, 0.6, 3.4, 0.24, 10), (170, 0.5, 2.6, 0.2, 15),
              (230, 0.62, 3.1, 0.22, 12), (300, 0.55, 2.8, 0.2, 14), (20, 1.05, 1.8, 0.16, 22), (80, 1.1, 2.2, 0.17, 20),
              (140, 1.0, 1.6, 0.15, 25), (200, 1.15, 2.0, 0.16, 22), (260, 1.05, 1.5, 0.14, 26), (330, 1.1, 1.9, 0.15, 24)]
    for ang, dist, h, r, tilt in spires:
        a, t = math.radians(ang), math.radians(tilt)
        base = mh.V(math.cos(a) * dist, math.sin(a) * dist, 0.1)
        axis = mh.V(math.cos(a) * math.sin(t), math.sin(a) * math.sin(t), math.cos(t))
        top = base + axis * h
        band = CORE if dist == 0.0 else ICE
        m.taper(band, base, top, r, r * 0.7, vertices=6)
        m.taper(band, top, top + axis * r * 2.5, r * 0.7, 0.0, vertices=6)
    m.save()


def torre_wayne():
    GLASS, WINDOWS, EMBLEM = 0, 1, 2
    m = mh.Model("torre_wayne", [(0.10, 0.11, 0.14), (1.0, 0.80, 0.40), (1.0, 0.95, 0.75)])
    tiers = [(1.2, 0.0, 3.0), (0.95, 3.0, 1.5), (0.7, 4.5, 1.0)]  # (ancho, base, alto)
    for w, z0, h in tiers:
        m.box(GLASS, (0, 0, z0 + h / 2), (w, w, h))
        # Franjas de ventanas encendidas alrededor de cada piso.
        z = z0 + 0.25
        while z < z0 + h - 0.1:
            m.box(WINDOWS, (0, 0, z), (w + 0.02, w + 0.02, 0.06))
            z += 0.32
    m.cone(GLASS, (0, 0, 6.1), 0.12, 0.0, 1.2, vertices=6)
    # La "W" en las cuatro caras del último piso: cuatro trazos inclinados.
    for face in range(4):
        rot = face * 90
        for k, (x, tilt) in enumerate([(-0.21, 18), (-0.07, -18), (0.07, 18), (0.21, -18)]):
            a = math.radians(rot)
            off = mh.V(x * math.cos(a) - 0.36 * math.sin(a), x * math.sin(a) + 0.36 * math.cos(a), 5.0)
            m.box(EMBLEM, off, (0.05, 0.02, 0.34), rot=(0, tilt, rot))
    m.save()


def dirigible():
    HULL, DARK, LIGHT, SCREEN = 0, 1, 2, 3
    m = mh.Model("dirigible", [(0.55, 0.56, 0.60), (0.12, 0.12, 0.14), (1.0, 0.2, 0.1), (0.3, 0.9, 1.0)])
    m.ball(HULL, (0, 0, 0), 1.0, scale=(0.6, 2.0, 0.6), segments=16, rings=10)
    # Aletas de la cola: dos verticales y dos horizontales.
    for z in (0.45, -0.45):
        m.box(DARK, (0, -1.7, z), (0.04, 0.6, 0.45))
    for x in (0.45, -0.45):
        m.box(DARK, (x, -1.7, 0), (0.45, 0.6, 0.04))
    m.box(DARK, (0, 0.2, -0.68), (0.25, 0.8, 0.2), bevel=0.04)  # góndola
    m.box(SCREEN, (0.6, 0, 0), (0.03, 1.4, 0.4))  # pantallas de anuncio a los lados
    m.box(SCREEN, (-0.6, 0, 0), (0.03, 1.4, 0.4))
    m.ball(LIGHT, (0, 2.0, 0), 0.08)  # luces de posición: nariz y aletas
    m.ball(LIGHT, (0, -1.95, 0.7), 0.06)
    m.save()


def ciudad_atlantis():
    PEARL, GOLD, GLOW = 0, 1, 2
    m = mh.Model("ciudad_atlantis", [(0.85, 0.90, 0.95), (0.85, 0.70, 0.35), (0.3, 1.0, 0.9)])
    m.disc(GOLD, (0, 0, 0.08), 1.6, 0.16, vertices=16)
    m.taper(PEARL, (0, 0, 0.16), (0, 0, 2.4), 0.24, 0.07, vertices=8)  # aguja central
    m.ball(GLOW, (0, 0, 2.5), 0.14, segments=8, rings=6)
    for k in range(6):  # anillo de torres con punta
        a = k / 6 * math.tau
        x, y = math.cos(a) * 0.95, math.sin(a) * 0.95
        h = 0.7 + 0.35 * (k % 3)
        m.taper(PEARL, (x, y, 0.16), (x, y, 0.16 + h), 0.12, 0.08, vertices=6)
        m.cone(GOLD, (x, y, 0.16 + h + 0.15), 0.1, 0.0, 0.3, vertices=6)
    for k in range(5):  # cúpulas pequeñas
        a = (k + 0.5) / 5 * math.tau
        m.ball(PEARL, (math.cos(a) * 0.55, math.sin(a) * 0.55, 0.16), 0.28, scale=(1, 1, 0.6), segments=10, rings=6)
    m.save()


def pez():
    BODY, FIN, EYE = 0, 1, 2
    m = mh.Model("pez", [(0.65, 0.75, 0.85), (1.0, 0.55, 0.15), (0.05, 0.05, 0.06)])
    m.ball(BODY, (0, 0, 0), 1.0, scale=(0.25, 0.8, 0.35), segments=10, rings=6)
    m.box(FIN, (0, -0.9, 0), (0.03, 0.3, 0.45), rot=(0, 0, 0))  # cola
    m.box(FIN, (0, 0, 0.36), (0.03, 0.35, 0.18))  # aleta de arriba
    for x in (0.2, -0.2):
        m.ball(EYE, (x, 0.5, 0.08), 0.05, segments=6, rings=4)
    m.save()


def star_labs():
    WHITE, WINDOWS, DOME, LOGO = 0, 1, 2, 3
    m = mh.Model("star_labs", [(0.85, 0.86, 0.88), (0.95, 0.85, 0.55), (0.35, 0.60, 0.95), (1.0, 0.15, 0.1)])
    m.disc(WHITE, (0, 0, 0.30), 1.2, 0.60, vertices=24)
    m.disc(WINDOWS, (0, 0, 0.40), 1.21, 0.12, vertices=24)
    m.ball(DOME, (0, 0, 0.60), 0.85, scale=(1, 1, 0.7), segments=16, rings=8)
    m.box(WHITE, (1.15, 0, 0.6), (0.5, 0.5, 1.2))  # torre lateral
    m.cone(WHITE, (1.15, 0, 1.45), 0.05, 0.0, 0.5, vertices=6)  # antena
    m.ball(LOGO, (1.15, 0.26, 0.95), 0.1, segments=8, rings=6)  # luz roja del logo
    m.save()


def puno_oa():
    GREEN = 0
    m = mh.Model("puno_oa", [(0.2, 1.0, 0.4)])
    m.box(GREEN, (0, 0, 0), (0.9, 0.7, 0.8), bevel=0.08)  # palma / dorso
    # Cuatro dedos doblados: nudillo adelante y la punta doblada hacia abajo.
    for k in range(4):
        x = -0.33 + 0.22 * k
        m.box(GREEN, (x, 0.45, 0.22), (0.2, 0.3, 0.26), bevel=0.04)
        m.box(GREEN, (x, 0.5, -0.12), (0.19, 0.22, 0.3), bevel=0.04)
    m.box(GREEN, (0.15, 0.42, -0.35), (0.5, 0.2, 0.18), rot=(0, 0, -10), bevel=0.04)  # pulgar
    m.box(GREEN, (0, -0.55, 0), (0.6, 0.5, 0.6), bevel=0.05)  # muñeca
    m.save()


for build in (templo, fortaleza, torre_wayne, dirigible, ciudad_atlantis, pez, star_labs, puno_oa):
    build()
mh.log("LISTO MUNDOS")
