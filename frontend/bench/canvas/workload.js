const COUNT = 2_500
const FRAMES = 60
const webglState = prepareWebgl()

globalThis.canvasBenchmark = Promise.resolve([
  measure('dom', renderDom),
  measure('canvas2d', renderCanvas2d),
  measure('webgl', renderWebgl),
])

function measure(renderer, render) {
  const samples = []
  for (let frame = 0; frame < FRAMES; frame += 1) {
    const start = performance.now()
    render(frame)
    samples.push(performance.now() - start)
  }
  samples.sort((a, b) => a - b)
  return { renderer, items: COUNT, frames: FRAMES, p50Ms: percentile(samples, 0.5), p95Ms: percentile(samples, 0.95) }
}

function renderDom(frame) {
  const root = document.querySelector('#dom')
  const fragment = document.createDocumentFragment()
  for (let index = 0; index < COUNT; index += 1) {
    const node = document.createElement('i')
    node.style.cssText = `position:absolute;transform:translate(${(index + frame) % 1280}px,${index % 720}px);width:2px;height:2px`
    fragment.append(node)
  }
  root.replaceChildren(fragment)
  void root.getBoundingClientRect()
}

function renderCanvas2d(frame) {
  const context = document.querySelector('#canvas2d').getContext('2d')
  context.clearRect(0, 0, 1280, 720)
  for (let index = 0; index < COUNT; index += 1) context.fillRect((index + frame) % 1280, index % 720, 2, 2)
}

function renderWebgl(frame) {
  if (!webglState) throw new Error('WebGL is unavailable')
  const { gl, positionBuffer, positions } = webglState
  for (let index = 0; index < COUNT; index += 1) {
    positions[index * 2] = (((index + frame) % 1280) / 640) - 1
    positions[index * 2 + 1] = 1 - ((index % 720) / 360)
  }
  gl.bindBuffer(gl.ARRAY_BUFFER, positionBuffer)
  gl.bufferSubData(gl.ARRAY_BUFFER, 0, positions)
  gl.clearColor((frame % 60) / 60, 0, 0, 1)
  gl.clear(gl.COLOR_BUFFER_BIT)
  gl.drawArrays(gl.POINTS, 0, COUNT)
  gl.finish()
}

function prepareWebgl() {
  const gl = document.querySelector('#webgl').getContext('webgl', { preserveDrawingBuffer: false })
  if (!gl) return null
  const vertex = shader(gl, gl.VERTEX_SHADER, 'attribute vec2 p; void main(){gl_Position=vec4(p,0.,1.);gl_PointSize=2.;}')
  const fragment = shader(gl, gl.FRAGMENT_SHADER, 'precision mediump float; void main(){gl_FragColor=vec4(1.,1.,1.,1.);}')
  const program = gl.createProgram()
  gl.attachShader(program, vertex)
  gl.attachShader(program, fragment)
  gl.linkProgram(program)
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program) ?? 'WebGL link failed')
  gl.useProgram(program)
  const positionBuffer = gl.createBuffer()
  gl.bindBuffer(gl.ARRAY_BUFFER, positionBuffer)
  const positions = new Float32Array(COUNT * 2)
  gl.bufferData(gl.ARRAY_BUFFER, positions.byteLength, gl.DYNAMIC_DRAW)
  const location = gl.getAttribLocation(program, 'p')
  gl.enableVertexAttribArray(location)
  gl.vertexAttribPointer(location, 2, gl.FLOAT, false, 0, 0)
  return { gl, positionBuffer, positions }
}

function shader(gl, type, source) {
  const value = gl.createShader(type)
  gl.shaderSource(value, source)
  gl.compileShader(value)
  if (!gl.getShaderParameter(value, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(value) ?? 'WebGL compile failed')
  return value
}

function percentile(sorted, fraction) {
  return Number(sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * fraction))].toFixed(3))
}
