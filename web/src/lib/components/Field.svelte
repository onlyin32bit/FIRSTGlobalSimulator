<script lang="ts">
  import { T } from '@threlte/core'
  import { useGltf } from '@threlte/extras'
  import { RigidBody, AutoColliders, Collider } from '@threlte/rapier'
  import { Vector3, Box3, type Mesh, type Object3D } from 'three'
  import { scores } from '$lib/scoreStore'

  let { anchors = $bindable({}) } = $props();

  const FIELD_OFFSET_Y = -5;
  const FIELD_OFFSET_X = -2;
  const FIELD_OFFSET_Z = 0;

  // Load the three separate GLBs
  const visualGltf    = useGltf('/models/FIELD/VISUAL/field.glb')
  const collisionGltf = useGltf('/models/FIELD/COLLISION/fieldcollision.glb')
  const anchorGltf    = useGltf('/models/FIELD/SEMANTICS/anchors/fieldanchors.glb')
  const zonesGltf     = useGltf('/models/FIELD/SEMANTICS/zones/fieldzones.glb')

  // Scoring zone definitions: mesh name → score key
  const scoringZones: Record<string, ScoreKey> = {
    'blueSUscore': 'blueSU',
    'redSUscore': 'redSU',
    'blueFSscore': 'blueFS',
    'redFSscore': 'redFS',
    'EXTscore': 'EXT',
  };

  type ScoreKey = 'blueSU' | 'redSU' | 'blueFS' | 'redFS' | 'EXT';

  // Store computed zone collider data (position + half-extents)
  interface ZoneColliderData {
    name: string;
    scoreKey: ScoreKey;
    position: [number, number, number];
    halfExtents: [number, number, number];
  }

  let zoneColliders = $state<ZoneColliderData[]>([]);

  // Extract anchor world positions and apply the same offsets
  anchorGltf.then(g => {
    const newAnchors: Record<string, [number, number, number]> = {};
    g.scene.traverse(node => {
      if (node.name && node.name !== 'Scene') {
        const pos = new Vector3();
        node.getWorldPosition(pos);
        newAnchors[node.name] = [pos.x + FIELD_OFFSET_X, pos.y + FIELD_OFFSET_Y, pos.z + FIELD_OFFSET_Z];
      }
    });
    anchors = newAnchors;
  });

  // Extract zone collider data from the zones GLB
  zonesGltf.then(g => {
    const result: ZoneColliderData[] = [];

    for (const [meshName, scoreKey] of Object.entries(scoringZones)) {
      const node = g.scene.getObjectByName(meshName);
      if (!node) continue;

      // Compute world-space bounding box of the mesh
      const box = new Box3().setFromObject(node);
      const center = new Vector3();
      const size = new Vector3();
      box.getCenter(center);
      box.getSize(size);

      // Ensure minimum sensor size so tiny meshes still detect balls
      const minSize = 0.1;
      
      result.push({
        name: meshName,
        scoreKey: scoreKey as ScoreKey,
        position: [center.x, center.y, center.z],
        halfExtents: [
          Math.max(size.x / 2, minSize),
          Math.max(size.y / 2, minSize),
          Math.max(size.z / 2, minSize),
        ],
      });
    }

    zoneColliders = result;
  });

  function handleScore(key: ScoreKey) {
    scores.update(s => ({ ...s, [key]: s[key] + 1 }));
  }
</script>

<!-- Single group applies the same offset to everything -->
<T.Group position={[FIELD_OFFSET_X, FIELD_OFFSET_Y, FIELD_OFFSET_Z]}>

  <!-- VISUAL: render the full visual GLB with all its hierarchy intact -->
  {#await visualGltf then g}
    <T is={g.scene} />
  {/await}

  <!-- COLLISION: trimesh preserves the full scene hierarchy so every box
       stays at its correct position/rotation/scale from the GLB -->
  {#await collisionGltf then g}
    <RigidBody type="fixed">
      <AutoColliders shape="trimesh">
        <T is={g.scene} visible={false} />
      </AutoColliders>
    </RigidBody>
  {/await}

  <!-- SCORING ZONES: individual sensor colliders per zone -->
  {#each zoneColliders as zone (zone.name)}
    <T.Group position={zone.position}>
      <RigidBody type="fixed">
        <Collider
          shape="cuboid"
          args={zone.halfExtents}
          sensor
          onsensorenter={() => handleScore(zone.scoreKey)}
        />
      </RigidBody>
    </T.Group>
  {/each}

</T.Group>
