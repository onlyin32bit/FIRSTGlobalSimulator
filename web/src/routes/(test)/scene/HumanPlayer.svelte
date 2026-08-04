<script lang="ts">
  import { T, useTask, useThrelte } from '@threlte/core'
  import { onMount } from 'svelte'
  import { Vector3 } from 'three'
  import { get } from 'svelte/store'
  import {
    humanPlayerCharge,
    humanPlayerThrow,
    humanPlayerGrabRequest,
    humanPlayerStorage,
    humanPlayerHeldPosition,
    handleLiftActive
  } from './stores'

  type HumanPlayerBounds = {
    minX: number
    maxX: number
    minZ: number
    maxZ: number
  }

  let {
    position = [-4.41658, 1.8, 2.99308] as [number, number, number],
    bounds = {
      minX: -5.196519,
      maxX: -3.636641,
      minZ: 2.320723,
      maxZ: 3.665437
    } as HumanPlayerBounds,
    fov = 75
  } = $props()
  const { camera } = useThrelte()
  const keys = { forward: false, back: false, left: false, right: false }
  let yaw = 0
  let pitch = 0
  let chargeTime = 0
  let isCharging = false
  let throwId = 0
  const MAX_CHARGE_TIME = 1.0
  let gamepadWasThrowing = false
  let gamepadWasGrabbing = false

  function requestGrab() {
    if (get(humanPlayerStorage) > 0) return

    const cam = camera.current
    if (!cam) return

    const direction = new Vector3()
    cam.getWorldDirection(direction).normalize()
    humanPlayerGrabRequest.update((request) => ({
      id: request.id + 1,
      origin: { x: cam.position.x, y: cam.position.y, z: cam.position.z },
      direction: { x: direction.x, y: direction.y, z: direction.z }
    }))
  }

  function throwBall() {
    if (get(humanPlayerStorage) <= 0) {
      isCharging = false
      chargeTime = 0
      humanPlayerCharge.set(0)
      return
    }

    const cam = camera.current
    if (cam) {
      const direction = new Vector3()
      cam.getWorldDirection(direction).normalize()
      throwId += 1
      humanPlayerThrow.set({
        id: throwId,
        origin: {
          x: cam.position.x + direction.x * 0.35,
          y: cam.position.y + direction.y * 0.35,
          z: cam.position.z + direction.z * 0.35
        },
        direction: { x: direction.x, y: direction.y, z: direction.z },
        power: Math.max(0.1, Math.min(1, chargeTime / MAX_CHARGE_TIME))
      })
    }

    isCharging = false
    chargeTime = 0
    humanPlayerCharge.set(0)
  }

  function setKey(event: KeyboardEvent, pressed: boolean) {
    const key = event.key.toLowerCase()
    if (key === 'e') {
      handleLiftActive.set(pressed)
      if (pressed && !event.repeat) requestGrab()
    }
    if (key === 'w') keys.forward = pressed
    if (key === 's') keys.back = pressed
    if (key === 'a') keys.left = pressed
    if (key === 'd') keys.right = pressed
    if (['w', 's', 'a', 'd'].includes(key)) event.preventDefault()
  }

  onMount(() => {
    const handleKeyDown = (event: KeyboardEvent) => setKey(event, true)
    const handleKeyUp = (event: KeyboardEvent) => setKey(event, false)
    const handleMouseMove = (event: MouseEvent) => {
      if (document.pointerLockElement !== document.body) return
      yaw -= event.movementX * 0.0025
      pitch = Math.max(-1.45, Math.min(1.45, pitch - event.movementY * 0.0025))
    }
    const handleMouseDown = (event: MouseEvent) => {
      if (event.button !== 0 || document.pointerLockElement !== document.body) return
      if (get(humanPlayerStorage) <= 0) {
        requestGrab()
      }
      isCharging = true
      chargeTime = 0
    }
    const handleMouseUp = (event: MouseEvent) => {
      if (event.button !== 0 || !isCharging) return
      throwBall()
    }
    const lockPointer = (event: MouseEvent) => {
      if ((event.target as HTMLElement | null)?.tagName === 'CANVAS') {
        document.body.requestPointerLock?.()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    window.addEventListener('keyup', handleKeyUp)
    window.addEventListener('mousemove', handleMouseMove)
    window.addEventListener('mousedown', handleMouseDown)
    window.addEventListener('mouseup', handleMouseUp)
    window.addEventListener('click', lockPointer)
    return () => {
      window.removeEventListener('keydown', handleKeyDown)
      window.removeEventListener('keyup', handleKeyUp)
      window.removeEventListener('mousemove', handleMouseMove)
      window.removeEventListener('mousedown', handleMouseDown)
      window.removeEventListener('mouseup', handleMouseUp)
      window.removeEventListener('click', lockPointer)
      if (document.pointerLockElement === document.body) document.exitPointerLock()
    }
  })

  function applyDeadzone(value: number, threshold = 0.15): number {
    if (Math.abs(value) < threshold) return 0
    const sign = Math.sign(value)
    const normalized = (Math.abs(value) - threshold) / (1 - threshold)
    return sign * Math.pow(normalized, 1.2)
  }

  useTask((delta) => {
    const cam = camera.current
    if (!cam) return

    const gamepads = navigator.getGamepads ? navigator.getGamepads() : []
    const gamepad = Array.from(gamepads).find((pad) => pad?.connected) ?? null
    const rawLeftX = gamepad?.axes[0] ?? 0
    const rawLeftY = gamepad?.axes[1] ?? 0
    const rawRightX = gamepad?.axes[2] ?? 0
    const rawRightY = gamepad?.axes[3] ?? 0
    const leftX = applyDeadzone(rawLeftX)
    const leftY = applyDeadzone(rawLeftY)
    const rightX = applyDeadzone(rawRightX)
    const rightY = applyDeadzone(rawRightY)
    const gamepadThrowing = gamepad?.buttons[0]?.pressed ?? false
    const gamepadGrabbing = gamepad?.buttons[2]?.pressed ?? false

    const JOYSTICK_SENSITIVITY = 1.2
    if (rightX !== 0) yaw -= rightX * delta * JOYSTICK_SENSITIVITY
    if (rightY !== 0) {
      // Gamepad Y is positive when pushed down; invert it so pushing up
      // raises the camera, matching normal first-person look controls.
      pitch = Math.max(-1.45, Math.min(1.45, pitch - rightY * delta * JOYSTICK_SENSITIVITY))
    }

    if (gamepadThrowing && !gamepadWasThrowing && !isCharging) {
      isCharging = true
      chargeTime = 0
    } else if (!gamepadThrowing && gamepadWasThrowing && isCharging) {
      throwBall()
    }
    gamepadWasThrowing = gamepadThrowing

    if (gamepadGrabbing && !gamepadWasGrabbing) requestGrab()
    gamepadWasGrabbing = gamepadGrabbing

    const gamepadLiftHandle = gamepad?.buttons[2]?.pressed || gamepad?.buttons[5]?.pressed || false
    if (gamepadLiftHandle) {
      handleLiftActive.set(true)
    }

    cam.rotation.set(pitch, yaw, 0, 'YXZ')

    const heldDirection = new Vector3()
    cam.getWorldDirection(heldDirection).normalize()
    humanPlayerHeldPosition.set({
      x: cam.position.x + heldDirection.x * 0.55,
      y: cam.position.y + heldDirection.y * 0.55,
      z: cam.position.z + heldDirection.z * 0.55
    })

    if (isCharging) {
      chargeTime = Math.min(MAX_CHARGE_TIME, chargeTime + delta)
      humanPlayerCharge.set(chargeTime / MAX_CHARGE_TIME)
    }

    // The field semantics arrive asynchronously. If the camera was created
    // at the fallback location, place it at the actual zone center once the
    // redHPzone bounds become available.
    const outsideZone =
      cam.position.x < bounds.minX || cam.position.x > bounds.maxX ||
      cam.position.z < bounds.minZ || cam.position.z > bounds.maxZ
    if (outsideZone) {
      cam.position.set(position[0], position[1], position[2])
    }

    const speed = 2.5 * delta
    const forward = Math.max(-1, Math.min(1, Number(keys.forward) - Number(keys.back) - leftY))
    const strafe = Math.max(-1, Math.min(1, Number(keys.right) - Number(keys.left) + leftX))

    // Move relative to the camera's current facing direction, not world axes.
    const cameraForward = new Vector3()
    cam.getWorldDirection(cameraForward)
    cameraForward.y = 0
    if (cameraForward.lengthSq() > 0.0001) cameraForward.normalize()
    const cameraRight = new Vector3().crossVectors(cameraForward, new Vector3(0, 1, 0)).normalize()
    cam.position.addScaledVector(cameraForward, forward * speed)
    cam.position.addScaledVector(cameraRight, strafe * speed)

    // Keep the human player inside the red human-player zone.
    cam.position.x = Math.max(bounds.minX, Math.min(bounds.maxX, cam.position.x))
    cam.position.z = Math.max(bounds.minZ, Math.min(bounds.maxZ, cam.position.z))
    cam.position.y = position[1]
  })
</script>

<T.PerspectiveCamera makeDefault {fov} position={position} near={0.01} far={1000}>
  {#if $humanPlayerStorage > 0}
    <T.Mesh position={[0.18, -0.16, -0.4 - chargeTime * 0.08]}>
      <T.SphereGeometry args={[0.05, 16, 16]} />
      <T.MeshStandardMaterial color="#f97316" roughness={0.8} metalness={0.1} />
    </T.Mesh>
  {/if}
</T.PerspectiveCamera>
