import * as THREE from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import * as fs from 'fs';
import * as path from 'path';

const loader = new GLTFLoader();
const filePath = path.resolve('static/models/FIELD/SEMANTICS/zones/fieldzones.glb');
const data = fs.readFileSync(filePath);
const arrayBuffer = data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength);

loader.parse(arrayBuffer, '', (gltf) => {
  console.log('=== Zone GLB Structure ===');
  gltf.scene.traverse((node) => {
    const indent = '  '.repeat(getDepth(node, gltf.scene));
    const type = (node as any).isMesh ? 'Mesh' : node.type;
    const pos = node.position;
    const scale = node.scale;
    console.log(`${indent}[${type}] "${node.name}" pos=(${pos.x.toFixed(3)}, ${pos.y.toFixed(3)}, ${pos.z.toFixed(3)}) scale=(${scale.x.toFixed(3)}, ${scale.y.toFixed(3)}, ${scale.z.toFixed(3)})`);
  });
}, (error) => {
  console.error('Error parsing GLB:', error);
});

function getDepth(node: THREE.Object3D, root: THREE.Object3D): number {
  let depth = 0;
  let current = node;
  while (current.parent && current !== root) {
    depth++;
    current = current.parent;
  }
  return depth;
}
