import { forwardRef, useImperativeHandle, useRef } from "react"
import { CameraControls } from "@react-three/drei"
import * as THREE from "three"

/** Imperative handle for the camera rig — lets the scene fly the camera to a
 *  specific 3D point with a smooth transition. */
export interface CameraRigHandle {
  /** Frame the whole scene at the given distance from origin. */
  frame: (distance: number) => void
  /** Smoothly move the camera so it looks at `target` from a distance scaled
   *  to the target node's radius. */
  focusOn: (target: THREE.Vector3, radius: number) => void
}

/** Wraps drei's `CameraControls` with a small imperative surface so the
 *  scene can focus on a selected node without owning camera math. */
export const CameraRig = forwardRef<CameraRigHandle>(function CameraRig(_, ref) {
  const controlsRef = useRef<CameraControls | null>(null)

  useImperativeHandle(
    ref,
    () => ({
      frame: (distance: number) => {
        const ctrl = controlsRef.current
        if (!ctrl) return
        ctrl.setLookAt(0, 0, distance, 0, 0, 0, false)
      },
      focusOn: (target: THREE.Vector3, radius: number) => {
        const ctrl = controlsRef.current
        if (!ctrl) return
        // Pull camera onto the same ray it currently looks down, but closer.
        const cam = ctrl.camera
        const dir = new THREE.Vector3().subVectors(cam.position, target).normalize()
        const distance = Math.max(radius * 16, 28)
        const newPos = target.clone().add(dir.multiplyScalar(distance))
        ctrl.setLookAt(newPos.x, newPos.y, newPos.z, target.x, target.y, target.z, true)
      },
    }),
    [],
  )

  return (
    <CameraControls
      ref={controlsRef}
      makeDefault
      smoothTime={0.32}
      draggingSmoothTime={0.12}
      minDistance={20}
      maxDistance={2400}
      dollySpeed={0.8}
      truckSpeed={1.0}
      polarRotateSpeed={0.7}
      azimuthRotateSpeed={0.7}
    />
  )
})
