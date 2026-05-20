import * as THREE from "three"

export function createNodeMaterial(color: string, emissive: string): THREE.MeshPhysicalMaterial {
  return new THREE.MeshPhysicalMaterial({
    color,
    emissive,
    emissiveIntensity: 0.7,
    roughness: 0.32,
    metalness: 0.08,
    clearcoat: 0.55,
    clearcoatRoughness: 0.25,
    reflectivity: 0.4,
    sheen: 0.4,
    sheenColor: new THREE.Color(color),
  })
}

export function createEdgeMaterial(color: string, opacity: number): THREE.LineBasicMaterial {
  return new THREE.LineBasicMaterial({
    color,
    opacity: Math.min(opacity, 1),
    transparent: true,
    linewidth: 1,
    fog: true,
  })
}

export function createLabelCanvas(
  text: string,
  fontSize: number = 16,
  backgroundColor: string = "rgba(0, 0, 0, 0.7)",
  textColor: string = "#ffffff"
): HTMLCanvasElement {
  const canvas = document.createElement("canvas")
  const context = canvas.getContext("2d")!

  // Set canvas size based on text
  canvas.width = 256
  canvas.height = 64

  // Draw background
  context.fillStyle = backgroundColor
  context.fillRect(0, 0, canvas.width, canvas.height)

  // Draw text
  context.fillStyle = textColor
  context.font = `${fontSize}px Arial`
  context.textAlign = "center"
  context.textBaseline = "middle"
  context.fillText(text, canvas.width / 2, canvas.height / 2)

  return canvas
}

export function createLabelTexture(canvas: HTMLCanvasElement): THREE.CanvasTexture {
  const texture = new THREE.CanvasTexture(canvas)
  texture.minFilter = THREE.LinearFilter
  texture.magFilter = THREE.LinearFilter
  return texture
}

export function createLabelMaterial(texture: THREE.CanvasTexture): THREE.MeshBasicMaterial {
  return new THREE.MeshBasicMaterial({
    map: texture,
    transparent: true,
    side: THREE.DoubleSide,
    fog: false,
  })
}

export function createGlowMaterial(color: string, intensity: number): THREE.MeshBasicMaterial {
  return new THREE.MeshBasicMaterial({
    color,
    transparent: true,
    opacity: Math.min(0.55 * intensity, 0.85),
    blending: THREE.AdditiveBlending,
    depthWrite: false,
    fog: false,
  })
}

export function createCommunityAnchorMaterial(color: string): THREE.MeshBasicMaterial {
  return new THREE.MeshBasicMaterial({
    color,
    opacity: 0.6,
    transparent: true,
    fog: true,
  })
}

export function updateNodeMaterialColor(
  material: THREE.MeshPhysicalMaterial,
  color: string,
  emissive: string,
  opacity: number
): void {
  material.color.setStyle(color)
  material.emissive.setStyle(emissive)
  material.opacity = opacity
  material.transparent = opacity < 1
  material.needsUpdate = true
}

export function updateEdgeMaterialColor(
  material: THREE.LineBasicMaterial,
  color: string,
  opacity: number
): void {
  material.color.setStyle(color)
  material.opacity = Math.min(opacity, 1)
  material.needsUpdate = true
}
