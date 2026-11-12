"""Genera los efectos de sonido del juego como archivos .wav (mono, 16 bits, 44100 Hz).

Todo sale de matematica: senos, ruido filtrado y envolventes. Con la misma SEMILLA siempre
salen los mismos archivos. Para correrlo desde la raiz del proyecto:

    python assets/sonidos/generar_sonidos.py

Los LOOPS (motor, turbo, warp y ambientes) son PERIODICOS: su final empata exacto con su
inicio, asi raylib los repite sin que se oiga un corte. Para lograrlo:
  - los senos dan un numero ENTERO de vueltas en lo que dura el loop,
  - el ruido se filtra con FFT, que trata la senal como circular (lo del final "toca" el inicio),
  - los eventos sueltos (burbujas, gotas, chispas) que se salen del final regresan al inicio.

Si quieren otro sonido, basta con reemplazar el .wav (mismo nombre) por uno propio.
"""

import pathlib
import sys
import wave

import numpy as np

SR = 44100
SEMILLA = 2026
CARPETA = pathlib.Path(__file__).resolve().parent
rng = np.random.default_rng(SEMILLA)
# Opcional: `python generar_sonidos.py boom_tube amb_apokolips` escribe solo esos.
SOLO = set(sys.argv[1:])


# ------------------------------------------------------------------------------------------
# Herramientas
# ------------------------------------------------------------------------------------------

def tiempo(dur):
    return np.arange(int(round(dur * SR))) / SR


def normal(x):
    """Escala la senal para que su pico valga 1."""
    pico = np.max(np.abs(x))
    return x / pico if pico > 0 else x


def ruido(n):
    return rng.standard_normal(n)


def filtro(x, bajo=0.0, alto=None):
    """Pasa banda suave hecho con FFT (circular: si x es un loop, el resultado tambien)."""
    n = len(x)
    espectro = np.fft.rfft(x)
    f = np.fft.rfftfreq(n, 1 / SR)
    g = np.ones_like(f)
    if bajo > 0:
        g *= 1 / (1 + (bajo / np.maximum(f, 1e-3)) ** 4)
    if alto:
        g *= 1 / (1 + (f / alto) ** 4)
    return np.fft.irfft(espectro * g, n)


def seno(f, t, fase=0.0):
    return np.sin(2 * np.pi * f * t + fase)


def barrido(dur, f0, f1, armonicos=(1.0,)):
    """Tono que se desliza de f0 a f1 (en escala exponencial, como lo oye el oido)."""
    t = tiempo(dur)
    f = f0 * (f1 / f0) ** (t / dur)
    fase = 2 * np.pi * np.cumsum(f) / SR
    return sum(a * np.sin(fase * (k + 1)) for k, a in enumerate(armonicos))


def ruido_barrido(dur, f0, f1, ancho=0.35):
    """Ruido cuyo "color" se desliza de f0 a f1: el whoosh de algo que pasa rapido.
    Se parte el ruido en bandas y cada banda suena mas cuando el centro pasa por ella."""
    t = tiempo(dur)
    base = ruido(len(t))
    fc = f0 * (f1 / f0) ** (t / dur)
    salida = np.zeros(len(t))
    for c in np.geomspace(60, 12000, 28):
        banda = filtro(base, c / 1.25, c * 1.25)
        salida += banda * np.exp(-(((np.log(c) - np.log(fc)) / ancho) ** 2))
    return salida


def campana(freq, dur, decaimiento=3.0):
    """Campana: parciales NO armonicos que se apagan a distinto ritmo (los agudos primero)."""
    t = tiempo(dur)
    parciales = [(1.0, 1.0, 1.0), (2.76, 0.45, 1.8), (5.40, 0.22, 3.0), (8.93, 0.10, 4.5)]
    s = sum(a * seno(freq * r, t) * np.exp(-decaimiento * k * t) for r, a, k in parciales)
    return s * np.minimum(t / 0.004, 1.0)


def poner(destino, fuente, en, envolver=False):
    """Suma `fuente` dentro de `destino` a partir del segundo `en`.
    Con envolver=True lo que se sale del final regresa al inicio (para loops)."""
    i = int(en * SR)
    if envolver:
        idx = (np.arange(len(fuente)) + i) % len(destino)
        np.add.at(destino, idx, fuente)
    else:
        j = min(len(destino), i + len(fuente))
        destino[i:j] += fuente[: j - i]


def bordes(x, entrada=0.003, salida=0.03):
    """Rampa corta al inicio y al final para que un efecto no "truene" al empezar o acabar."""
    x = x.copy()
    a, b = int(entrada * SR), int(salida * SR)
    if a:
        x[:a] *= np.linspace(0, 1, a)
    if b:
        x[-b:] *= np.linspace(1, 0, b)
    return x


def chispa(dur):
    """Chasquido electrico: rafaga de ruido agudo que se apaga muy rapido."""
    t = tiempo(dur)
    return filtro(ruido(len(t)), 2500, 9000) * np.exp(-t / (dur * 0.3))


def guardar(nombre, x, pico=0.9, loop=False):
    # Con nombres en la linea de comandos solo se escriben esos (los demas se calculan igual,
    # asi la secuencia al azar no cambia). Sirve si el juego esta abierto usando los otros.
    if SOLO and nombre not in SOLO:
        return
    x = normal(x) * pico
    if not loop:
        x = bordes(x)
    datos = (np.clip(x, -1, 1) * 32767).astype("<i2")
    with wave.open(str(CARPETA / f"{nombre}.wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(datos.tobytes())
    print(f"{nombre}.wav  {len(x) / SR:.2f} s")


# ------------------------------------------------------------------------------------------
# LOOPS de la nave (el juego les cambia volumen y tono segun la velocidad)
# ------------------------------------------------------------------------------------------

def motor():
    # 2 s: 55 Hz y sus armonicos dan vueltas enteras; el temblor de 4 Hz tambien (8 vueltas).
    t = tiempo(2.0)
    zumbido = sum(a * seno(55 * k, t) for k, a in [(1, 1.0), (2, 0.6), (3, 0.35), (4, 0.2), (6, 0.08)])
    rumor = filtro(ruido(len(t)), 20, 300)
    return (normal(zumbido) * 0.8 + normal(rumor) * 0.5) * (1 + 0.12 * seno(4, t))


def turbo():
    t = tiempo(2.0)
    rugido = normal(filtro(ruido(len(t)), 30, 700))
    silbido = normal(filtro(ruido(len(t)), 1500, 6000)) * (0.6 + 0.4 * seno(12, t))
    zumbido = normal(sum(a * seno(82.5 * k, t) for k, a in [(1, 1.0), (2, 0.5), (3, 0.3)]))
    return rugido + silbido * 0.35 + zumbido * 0.4


def alarma_calor():
    # 2 s: cuatro pitidos (agudo, grave, agudo, grave). Cada pitido arranca y termina en 0,
    # asi que el loop cierra solo. Las frecuencias dan vueltas enteras en cada pitido.
    salida = np.zeros(int(2.0 * SR))
    t = tiempo(0.3)
    envolvente = np.minimum(np.minimum(t / 0.01, 1), np.minimum((0.3 - t) / 0.03, 1))
    for k, f in enumerate([880.0, 660.0, 880.0, 660.0]):
        pitido = sum(seno(f * n, t) / n for n in (1, 3, 5)) * envolvente  # casi cuadrada
        poner(salida, pitido, 0.5 * k)
    return salida


def warp_crucero():
    # 4 s. Los pares 220/220.25 Hz "baten" 1 vez por loop: un brillo que respira.
    t = tiempo(4.0)
    aire = normal(filtro(ruido(len(t)), 900, 7000)) * (0.75 + 0.25 * seno(0.5, t))
    tonos = normal(seno(220, t) + seno(220.25, t) + 0.6 * (seno(330, t) + seno(330.25, t)) + 0.3 * (seno(440, t) + seno(440.5, t)))
    grave = normal(filtro(ruido(len(t)), 30, 150))
    return aire * 0.7 + tonos * 0.35 + grave * 0.4


# ------------------------------------------------------------------------------------------
# AMBIENTES de cada mundo (4 s cada uno; se oyen al acercarse)
# ------------------------------------------------------------------------------------------

def amb_sol():
    t = tiempo(4.0)
    grave = normal(filtro(ruido(len(t)), 15, 110)) * (0.7 + 0.3 * seno(0.5, t))
    latido = seno(40, t) * (0.6 + 0.4 * seno(0.25, t))
    fuego = normal(filtro(ruido(len(t)), 200, 1200))
    return grave + latido * 0.6 + fuego * 0.15


def amb_themyscira():
    # Olas del mar: una ola grande cada 4 s y otra mas chica a la mitad.
    t = tiempo(4.0)
    ola1 = ((1 - np.cos(2 * np.pi * t / 4)) / 2) ** 3
    ola2 = ((1 - np.cos(2 * np.pi * (t - 2) / 4)) / 2) ** 3 * 0.7
    olas = normal(filtro(ruido(len(t)), 80, 1200)) * (ola1 + ola2)
    espuma = normal(filtro(ruido(len(t)), 2000, 8000)) * (ola1**2 + ola2**2)
    viento = normal(filtro(ruido(len(t)), 250, 700)) * (0.7 + 0.3 * seno(0.25, t))
    return olas + espuma * 0.4 + viento * 0.15


def amb_krypton():
    # Zumbido de cristal (do, sol, do agudo, cada uno con un gemelo que bate) + campanitas.
    t = tiempo(4.0)
    tonos = sum(a * (seno(f, t) + seno(f + 0.25, t)) for f, a in [(523.25, 1.0), (784.0, 0.6), (1046.5, 0.4)])
    salida = normal(tonos) * 0.5 * (0.8 + 0.2 * seno(0.5, t))
    for _ in range(5):
        f = rng.choice([2093.0, 2637.0, 3136.0])
        poner(salida, campana(f, 0.9, 4.0) * rng.uniform(0.15, 0.3), rng.uniform(0, 4), envolver=True)
    return salida


def amb_gotham():
    # Lluvia, gotas sueltas, el rumor de la ciudad y una sirena a lo lejos.
    t = tiempo(4.0)
    n = len(t)
    lluvia = normal(filtro(ruido(n), 1500, 9000))
    gotas = np.zeros(n)
    gotas[rng.integers(0, n, 140)] = rng.uniform(-1, 1, 140)
    gotas = normal(filtro(gotas, 1000, 6000))
    ciudad = normal(filtro(ruido(n), 30, 200))
    # Sirena: f(t) = 780 + 170 sen(2 pi t / 2). Su fase se integra a mano para que el loop cierre.
    fase = 2 * np.pi * 780 * t - 170 * 2 * np.cos(2 * np.pi * t / 2)
    sirena = normal(filtro(np.sin(fase) + 0.3 * np.sin(2 * fase), 0, 1500))
    return lluvia * 0.5 + gotas * 0.4 + ciudad * 0.3 + sirena * 0.12


def amb_atlantis():
    # Bajo el agua: todo apagado, burbujas que suben y el canto de una ballena.
    t = tiempo(4.0)
    profundo = normal(filtro(ruido(len(t)), 20, 350)) * (0.8 + 0.2 * seno(0.25, t))
    salida = profundo * 0.6
    for _ in range(18):
        f0 = rng.uniform(350, 1000)
        burbuja = barrido(0.07, f0, f0 * 1.8) * np.exp(-tiempo(0.07) * 50)
        poner(salida, burbuja * rng.uniform(0.2, 0.45), rng.uniform(0, 4), envolver=True)
    canto = filtro(barrido(1.4, 180, 260, (1.0, 0.3)) * np.sin(np.pi * tiempo(1.4) / 1.4), 0, 900)
    poner(salida, normal(canto) * 0.3, 1.2, envolver=True)
    return salida


def amb_oa():
    # Energia verde: un zumbido grave que pulsa 1 vez por segundo y un brillo agudo.
    t = tiempo(4.0)
    pulso = ((1 + seno(1, t)) / 2) ** 2
    zumbido = normal(seno(110, t) + 0.5 * seno(220, t) + 0.3 * seno(330, t) + 0.15 * seno(440, t)) * (0.65 + 0.35 * pulso)
    brillo = normal(seno(1760, t) + seno(1760.5, t)) * (0.5 + 0.5 * seno(2, t))
    aire = normal(filtro(ruido(len(t)), 2000, 5000))
    return zumbido + brillo * 0.12 + aire * 0.08


def amb_central_city():
    # Zumbido electrico de 120 Hz, trafico de la ciudad y chispazos sueltos.
    t = tiempo(4.0)
    zumbido = normal(sum(seno(120 * k, t) / k for k in (1, 3, 5, 7)))
    trafico = normal(filtro(ruido(len(t)), 40, 300))
    salida = zumbido * 0.35 + trafico * 0.3
    for _ in range(25):
        poner(salida, normal(chispa(rng.uniform(0.005, 0.02))) * rng.uniform(0.1, 0.35), rng.uniform(0, 4), envolver=True)
    return salida


# ------------------------------------------------------------------------------------------
# EFECTOS sueltos
# ------------------------------------------------------------------------------------------

def warp_carga():
    # Dura lo mismo que la fase de alineacion del warp (1 s): sube hasta el salto.
    t = tiempo(1.0)
    tono = normal(barrido(1.0, 120, 900, (1.0, 0.5, 0.25)))
    aire = normal(ruido_barrido(1.0, 300, 3000))
    return (tono * 0.6 + aire * 0.5) * (t / 1.0) ** 1.5


def warp_salto():
    t = tiempo(1.3)
    golpe = barrido(1.3, 70, 35) * np.exp(-t * 4)
    whoosh = normal(ruido_barrido(1.3, 6000, 200)) * np.exp(-t * 2.5)
    return normal(golpe) + whoosh * 0.8


def warp_llegada():
    t = tiempo(1.3)
    whoosh = normal(ruido_barrido(1.3, 3000, 150)) * np.exp(-t * 2.5)
    tono = normal(barrido(1.3, 700, 150, (1.0, 0.4))) * np.exp(-t * 3)
    asentar = seno(55, t) * np.exp(-t * 3) * np.minimum(t / 0.05, 1)
    return whoosh * 0.8 + tono * 0.4 + asentar * 0.5


def warp_cancelar():
    t = tiempo(0.7)
    apagado = normal(barrido(0.7, 600, 80, (1.0, 0.5, 0.3))) * np.exp(-t * 4)
    salida = apagado.copy()
    for _ in range(4):
        poner(salida, normal(chispa(0.015)) * 0.25, rng.uniform(0, 0.4))
    return salida


def dos_tonos(f1, f2):
    """Dos notas de campanita seguidas (enganchar / soltar la orbita)."""
    salida = np.zeros(int(0.4 * SR))
    poner(salida, campana(f1, 0.35, 6.0), 0.0)
    poner(salida, campana(f2, 0.3, 6.0), 0.11)
    poner(salida, normal(chispa(0.004)) * 0.3, 0.0)
    return salida


def choque():
    t = tiempo(0.6)
    golpe = barrido(0.6, 55, 35) * np.exp(-t * 9)
    polvo = normal(filtro(ruido(len(t)), 40, 900)) * np.exp(-t * 18)
    metal = campana(180, 0.6, 7.0)
    return normal(golpe) + polvo * 0.7 + normal(metal) * 0.25


def escaneo():
    t = tiempo(1.1)
    radar = normal(barrido(1.1, 300, 1500)) * (0.6 + 0.4 * seno(30, t)) * np.where(t < 0.85, 0.5, 0.0)
    salida = radar
    poner(salida, normal(campana(1760, 0.25, 8.0)) * 0.8, 0.85)
    return salida


def cristal():
    return campana(1318.5, 0.9, 3.0)  # mi agudo; el juego le sube el tono con cada cristal


def cristales_completos():
    salida = np.zeros(int(2.2 * SR))
    notas = [1046.5, 1318.5, 1568.0, 2093.0]  # do, mi, sol, do: acorde mayor
    for k, f in enumerate(notas):
        poner(salida, campana(f, 0.6, 4.0) * 0.6, 0.15 * k)
    for f in notas:
        poner(salida, campana(f, 1.5, 1.5) * 0.45, 0.7)
    brillo = normal(filtro(ruido(int(1.5 * SR)), 5000, 11000)) * np.exp(-tiempo(1.5) * 3) * 0.15
    poner(salida, brillo, 0.7)
    return salida


def destino():
    t = tiempo(0.15)
    return (seno(880, t) + 0.3 * seno(1760, t)) * np.exp(-t * 25)


def foto():
    # Obturador: dos clics y un zumbido mecanico corto entre ellos.
    salida = np.zeros(int(0.3 * SR))
    clic = normal(chispa(0.006))
    poner(salida, clic, 0.0)
    poner(salida, clic * 0.8, 0.12)
    poner(salida, normal(filtro(ruido(int(0.08 * SR)), 300, 1500)) * 0.2, 0.02)
    return salida


def interruptor():
    t = tiempo(0.06)
    salida = seno(2000, t) * np.exp(-t * 200)
    poner(salida, normal(chispa(0.004)) * 0.3, 0.0)
    return salida


def error():
    # "No se puede": dos zumbidos graves (onda casi cuadrada: solo armonicos impares).
    salida = np.zeros(int(0.35 * SR))
    t = tiempo(0.12)
    zumbido = sum(seno(140 * k, t) / k for k in (1, 3, 5, 7)) * np.minimum(t / 0.005, 1)
    poner(salida, bordes(zumbido, 0.003, 0.02), 0.0)
    poner(salida, bordes(zumbido, 0.003, 0.02), 0.17)
    return salida


def explosion():
    # 3 s: golpe grave que cae, whoosh del fuego de agudo a grave, y pedazos que truenan despues.
    t = tiempo(3.0)
    golpe = barrido(3.0, 90, 25, (1.0, 0.5)) * np.exp(-t * 2.2)
    fuego = normal(ruido_barrido(3.0, 4000, 120, 0.5)) * np.exp(-t * 1.4)
    rugido = normal(filtro(ruido(len(t)), 20, 250)) * np.exp(-t * 1.0)
    salida = normal(golpe) + fuego * 0.8 + rugido * 0.7
    for _ in range(14):
        poner(salida, normal(chispa(rng.uniform(0.01, 0.04))) * rng.uniform(0.1, 0.3), rng.uniform(0.3, 2.2))
    return salida * np.minimum(t / 0.004, 1)


def boom_tube():
    # 3.5 s: el "BUM" grave del portal, un tunel de viento que sube, un acorde que vibra
    # mientras esta abierto y al final el viento que baja cuando se cierra.
    salida = np.zeros(int(3.5 * SR))
    t = tiempo(3.5)
    bum = barrido(1.5, 55, 28, (1.0, 0.4)) * np.exp(-tiempo(1.5) * 3)
    poner(salida, normal(bum), 0.0)
    poner(salida, normal(ruido_barrido(1.2, 200, 6000)) * np.linspace(0, 1, int(1.2 * SR)) * 0.6, 0.0)
    vibrato = 1 + 0.004 * seno(6, t)
    fase = 2 * np.pi * np.cumsum(vibrato) / SR
    acorde = sum(np.sin(fase * f) for f in (220.0, 277.2, 329.6, 440.0))
    abierto = np.clip((t - 0.3) / 0.4, 0, 1) * np.clip((3.3 - t) / 0.6, 0, 1)
    salida += normal(acorde) * abierto * 0.35
    poner(salida, normal(ruido_barrido(1.0, 5000, 150)) * np.linspace(1, 0, int(1.0 * SR)) * 0.5, 2.4)
    return salida


def amb_apokolips():
    # 4 s: zumbido de fabricas, fuego que chisporrotea y tambores de guerra (uno por segundo).
    t = tiempo(4.0)
    n = len(t)
    fabrica = normal(sum(a * seno(55 * k, t) for k, a in [(1, 1.0), (2, 0.5), (3, 0.4), (5, 0.15)])) * (0.75 + 0.25 * seno(0.5, t))
    fuego = normal(filtro(ruido(n), 150, 2500)) * (0.7 + 0.3 * seno(1.25, t))
    salida = fabrica * 0.45 + fuego * 0.35
    golpe = barrido(0.35, 70, 38) * np.exp(-tiempo(0.35) * 9)
    for k in range(4):
        poner(salida, normal(golpe) * (0.9 if k % 2 == 0 else 0.6), float(k), envolver=True)
    for _ in range(30):
        poner(salida, normal(chispa(rng.uniform(0.004, 0.015))) * rng.uniform(0.08, 0.25), rng.uniform(0, 4), envolver=True)
    return salida


def rayo():
    # Descarga de Central City: chisporroteo que se corta a pedazos + un trueno chico.
    t = tiempo(0.7)
    corte = (rng.random(len(t) // 220 + 1) > 0.45).repeat(220)[: len(t)]
    chispas = normal(filtro(ruido(len(t)), 1500, 9000)) * corte * np.exp(-t * 6)
    silbido = normal(barrido(0.7, 3000, 300)) * np.exp(-t * 8)
    trueno = normal(filtro(ruido(len(t)), 30, 200)) * np.exp(-t * 5)
    return chispas + silbido * 0.3 + trueno * 0.5


LOOPS = [motor, turbo, warp_crucero, alarma_calor, amb_sol, amb_themyscira, amb_krypton, amb_gotham, amb_atlantis, amb_oa, amb_central_city]
EFECTOS = [warp_carga, warp_salto, warp_llegada, warp_cancelar, choque, escaneo, cristal, cristales_completos, destino, foto, interruptor, error, rayo]

if __name__ == "__main__":
    for generar in LOOPS:
        guardar(generar.__name__, generar(), pico=0.7, loop=True)
    for generar in EFECTOS:
        guardar(generar.__name__, generar())
    guardar("orbita_enganchar", dos_tonos(660, 990))
    guardar("orbita_soltar", dos_tonos(990, 660))
    # Los nuevos van al final: todos comparten la secuencia de numeros al azar, asi los de
    # arriba siguen saliendo identicos aunque se agreguen sonidos.
    guardar("explosion", explosion())
    guardar("boom_tube", boom_tube())
    guardar("amb_apokolips", amb_apokolips(), pico=0.7, loop=True)
