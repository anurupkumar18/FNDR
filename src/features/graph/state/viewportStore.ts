import { create } from "zustand"
import * as THREE from "three"

/** Camera + canvas state needed to project 3D world coords to 2D screen
 *  coords from outside the R3F Canvas (so the GraphLabels DOM overlay can
 *  position itself over each node). Pushed every frame by ViewportSync. */
interface ViewportState {
  /** Combined projection * inverse-world matrix; multiplied with a vec3 gives
   *  clip space. Wrapped in a stable Matrix4 instance — we mutate it in place
   *  to avoid allocating per frame. */
  matrix: THREE.Matrix4
  /** Canvas pixel size at the last update. */
  width: number
  height: number
  /** Counter that increments each frame — components can subscribe to this
   *  one number when they only care about cache-busting (not the matrix). */
  tick: number
  /** Imperative setter — call from inside Canvas via useFrame. */
  set: (camera: THREE.Camera, w: number, h: number) => void
}

export const useViewportStore = create<ViewportState>((set, get) => ({
  matrix: new THREE.Matrix4(),
  width: 0,
  height: 0,
  tick: 0,
  set: (camera, w, h) => {
    const m = get().matrix
    m.multiplyMatrices(camera.projectionMatrix, camera.matrixWorldInverse)
    set({ width: w, height: h, tick: get().tick + 1 })
  },
}))
