// WebGL sampler that reprojects an equirectangular frame the way the render
// stage's `v360` filter does: the same output projections, the same yaw/pitch/
// roll frame (+X right, +Y up, +Z forward) and the same vertical field of view
// derived from the output aspect. It exists so the reframe viewport shows the
// real framing instead of a decorative overlay.
//
// It is loaded on demand (`import()` from the viewport) so the WebGL code never
// costs anything for a project that does not touch 360 footage.
//
// Scope: equirectangular INPUT only. Fisheye and dual-fisheye sources need the
// lens geometry of the specific camera, which the client does not have, so the
// viewport falls back to its wireframe indicator for them rather than showing a
// plausible-looking frame that the render would not reproduce.

import type { OutputProjection } from '../../types'

export interface SamplerView {
  /** Degrees. */
  yaw: number
  pitch: number
  roll: number
  /** Horizontal field of view in degrees; the vertical one follows the aspect. */
  fov: number
  projection: OutputProjection
}

export interface EquirectSampler {
  draw(frame: TexImageSource, view: SamplerView): void
  dispose(): void
}

/** Projection ids shared with the fragment shader's `uProjection`. */
const PROJECTION_IDS: Record<OutputProjection, number> = {
  flat: 0,
  equirect: 1,
  fisheye: 2,
  stereographic: 3,
  pannini: 4,
}

const VERTEX_SHADER = `
attribute vec2 aCorner;
varying vec2 vPos;
void main() {
  vPos = aCorner;
  gl_Position = vec4(aCorner, 0.0, 1.0);
}
`

const FRAGMENT_SHADER = `
precision highp float;
varying vec2 vPos;
uniform sampler2D uTex;
uniform vec2 uHalfFov;
uniform vec3 uRot;
uniform int uProjection;

const float PI = 3.141592653589793;

vec3 fromSphere(float theta, float phi) {
  return vec3(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta));
}

vec3 fromLonLat(float lon, float lat) {
  return vec3(cos(lat) * sin(lon), sin(lat), cos(lat) * cos(lon));
}

vec3 outputDirection(vec2 p) {
  float ax = uHalfFov.x;
  float ay = uHalfFov.y;
  if (uProjection == 1) {
    return fromLonLat(p.x * PI, p.y * PI * 0.5);
  }
  if (uProjection == 2) {
    vec2 a = vec2(p.x * ax, p.y * ay);
    return fromSphere(length(a), atan(a.y, a.x));
  }
  if (uProjection == 3) {
    vec2 q = vec2(p.x * 2.0 * tan(ax * 0.5), p.y * 2.0 * tan(ay * 0.5));
    return fromSphere(2.0 * atan(length(q) * 0.5), atan(q.y, q.x));
  }
  if (uProjection == 4) {
    float lon = 2.0 * atan(p.x * tan(ax * 0.5));
    return fromLonLat(lon, atan(p.y * tan(ay) * (1.0 + cos(lon)) * 0.5));
  }
  return normalize(vec3(p.x * tan(ax), p.y * tan(ay), 1.0));
}

vec3 look(vec3 v) {
  float cy = cos(uRot.x), sy = sin(uRot.x);
  float cp = cos(uRot.y), sp = sin(uRot.y);
  float cr = cos(uRot.z), sr = sin(uRot.z);
  vec3 rolled = vec3(v.x * cr - v.y * sr, v.x * sr + v.y * cr, v.z);
  vec3 pitched = vec3(rolled.x, rolled.y * cp + rolled.z * sp, -rolled.y * sp + rolled.z * cp);
  return vec3(pitched.x * cy + pitched.z * sy, pitched.y, -pitched.x * sy + pitched.z * cy);
}

void main() {
  vec3 direction = normalize(look(outputDirection(vPos)));
  float lon = atan(direction.x, direction.z);
  float lat = asin(clamp(direction.y, -1.0, 1.0));
  gl_FragColor = texture2D(uTex, vec2(lon / (2.0 * PI) + 0.5, 0.5 - lat / PI));
}
`

function compile(gl: WebGLRenderingContext, type: number, source: string): WebGLShader | null {
  const shader = gl.createShader(type)
  if (!shader) return null
  gl.shaderSource(shader, source)
  gl.compileShader(shader)
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    gl.deleteShader(shader)
    return null
  }
  return shader
}

function link(gl: WebGLRenderingContext): WebGLProgram | null {
  const vertex = compile(gl, gl.VERTEX_SHADER, VERTEX_SHADER)
  const fragment = compile(gl, gl.FRAGMENT_SHADER, FRAGMENT_SHADER)
  if (!vertex || !fragment) return null
  const program = gl.createProgram()
  if (!program) return null
  gl.attachShader(program, vertex)
  gl.attachShader(program, fragment)
  gl.linkProgram(program)
  gl.deleteShader(vertex)
  gl.deleteShader(fragment)
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    gl.deleteProgram(program)
    return null
  }
  return program
}

const RADIANS = Math.PI / 180

/**
 * Build a sampler over `canvas`, or `null` when this browser has no usable
 * WebGL context. The caller falls back to the wireframe indicator.
 */
export function createEquirectSampler(canvas: HTMLCanvasElement): EquirectSampler | null {
  const gl =
    (canvas.getContext('webgl', { alpha: false, antialias: false }) as WebGLRenderingContext | null) ??
    (canvas.getContext('experimental-webgl') as WebGLRenderingContext | null)
  if (!gl) return null

  const program = link(gl)
  const buffer = gl.createBuffer()
  const texture = gl.createTexture()
  if (!program || !buffer || !texture) {
    gl.getExtension('WEBGL_lose_context')?.loseContext()
    return null
  }

  gl.bindBuffer(gl.ARRAY_BUFFER, buffer)
  // One oversized triangle covers the clip space without an index buffer.
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW)
  const corner = gl.getAttribLocation(program, 'aCorner')
  gl.enableVertexAttribArray(corner)
  gl.vertexAttribPointer(corner, 2, gl.FLOAT, false, 0, 0)

  gl.bindTexture(gl.TEXTURE_2D, texture)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR)
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR)

  gl.useProgram(program)
  const uniforms = {
    tex: gl.getUniformLocation(program, 'uTex'),
    halfFov: gl.getUniformLocation(program, 'uHalfFov'),
    rot: gl.getUniformLocation(program, 'uRot'),
    projection: gl.getUniformLocation(program, 'uProjection'),
  }
  gl.uniform1i(uniforms.tex, 0)

  let disposed = false

  return {
    draw(frame, view) {
      if (disposed || gl.isContextLost()) return
      const width = Math.max(1, canvas.width)
      const height = Math.max(1, canvas.height)
      gl.viewport(0, 0, width, height)
      gl.activeTexture(gl.TEXTURE0)
      gl.bindTexture(gl.TEXTURE_2D, texture)
      gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false)
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB, gl.RGB, gl.UNSIGNED_BYTE, frame)

      // The render stage derives the vertical field of view from the output
      // aspect; matching it here is what keeps the framing identical.
      const horizontal = Math.max(1, Math.min(360, view.fov)) * RADIANS
      const vertical = Math.max(1 * RADIANS, Math.min(360 * RADIANS, (horizontal * height) / width))
      gl.useProgram(program)
      gl.uniform2f(uniforms.halfFov, horizontal / 2, vertical / 2)
      gl.uniform3f(
        uniforms.rot,
        view.yaw * RADIANS,
        view.pitch * RADIANS,
        view.roll * RADIANS,
      )
      gl.uniform1i(uniforms.projection, PROJECTION_IDS[view.projection] ?? 0)
      gl.drawArrays(gl.TRIANGLES, 0, 3)
    },
    dispose() {
      if (disposed) return
      disposed = true
      gl.deleteTexture(texture)
      gl.deleteBuffer(buffer)
      gl.deleteProgram(program)
      gl.getExtension('WEBGL_lose_context')?.loseContext()
    },
  }
}
