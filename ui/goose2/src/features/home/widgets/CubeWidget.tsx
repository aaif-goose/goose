import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import cubeImage1 from "@/assets/home/cube/1.png";
import cubeImage2 from "@/assets/home/cube/2.png";
import cubeImage3 from "@/assets/home/cube/3.png";
import cubeImage4 from "@/assets/home/cube/4.png";
import cubeImage5 from "@/assets/home/cube/5.png";
import type { WidgetRenderProps } from "./types";

const THREE_MODULE_URL = "/vendor/three/three.module.js";
const ROUNDED_BOX_MODULE_URL = "/vendor/three/RoundedBoxGeometry.js";
const ORTHO_SIZE = 1.4;

type ThreeVector3 = {
  set: (x: number, y: number, z: number) => ThreeVector3;
  copy: (value: ThreeVector3) => ThreeVector3;
  applyMatrix4: (matrix: ThreeMatrix4) => ThreeVector3;
  setFromMatrixPosition: (matrix: unknown) => ThreeVector3;
};

type ThreeMatrix4 = {
  copy: (matrix: unknown) => ThreeMatrix4;
  invert: () => ThreeMatrix4;
};

type ThreeTexture = {
  wrapS: unknown;
  wrapT: unknown;
  colorSpace: unknown;
  minFilter: unknown;
  magFilter: unknown;
  dispose: () => void;
};

type CubeUniforms = {
  uTime: { value: number };
  uCamObj: { value: ThreeVector3 };
  uTexA: { value: ThreeTexture };
  uTexB: { value: ThreeTexture };
  uTexC: { value: ThreeTexture };
  uTexD: { value: ThreeTexture };
  uTexE: { value: ThreeTexture };
};

type ThreeShaderMaterial = {
  uniforms: CubeUniforms;
  dispose: () => void;
};

type ThreeGeometry = {
  dispose: () => void;
};

type ThreeMesh = {
  matrixWorld: unknown;
  rotation: { x: number; y: number };
  updateMatrixWorld: () => void;
};

type ThreeScene = {
  add: (object: ThreeMesh) => void;
};

type ThreeCamera = {
  left: number;
  right: number;
  top: number;
  bottom: number;
  matrixWorld: unknown;
  position: ThreeVector3;
  lookAt: (x: number, y: number, z: number) => void;
  updateProjectionMatrix: () => void;
};

type ThreeRenderer = {
  domElement: HTMLCanvasElement;
  outputColorSpace: unknown;
  toneMapping: unknown;
  setPixelRatio: (value: number) => void;
  setSize: (width: number, height: number, updateStyle?: boolean) => void;
  setClearColor: (color: number, alpha?: number) => void;
  render: (scene: ThreeScene, camera: ThreeCamera) => void;
  dispose: () => void;
};

type ThreeTextureLoader = {
  setCrossOrigin: (value: string) => void;
  load: (url: string) => ThreeTexture;
};

type ThreeClock = {
  getElapsedTime: () => number;
};

type ThreeRuntime = {
  WebGLRenderer: new (options: {
    antialias: boolean;
    alpha: boolean;
  }) => ThreeRenderer;
  Scene: new () => ThreeScene;
  OrthographicCamera: new (
    left: number,
    right: number,
    top: number,
    bottom: number,
    near: number,
    far: number,
  ) => ThreeCamera;
  TextureLoader: new () => ThreeTextureLoader;
  ShaderMaterial: new (options: {
    transparent: boolean;
    depthWrite: boolean;
    uniforms: CubeUniforms;
    vertexShader: string;
    fragmentShader: string;
  }) => ThreeShaderMaterial;
  Mesh: new (
    geometry: ThreeGeometry,
    material: ThreeShaderMaterial,
  ) => ThreeMesh;
  Clock: new () => ThreeClock;
  Matrix4: new () => ThreeMatrix4;
  Vector3: new () => ThreeVector3;
  ClampToEdgeWrapping: unknown;
  LinearFilter: unknown;
  LinearMipmapLinearFilter: unknown;
  NoToneMapping: unknown;
  SRGBColorSpace: unknown;
};

type RoundedBoxModule = {
  RoundedBoxGeometry: new (
    width: number,
    height: number,
    depth: number,
    segments: number,
    radius: number,
  ) => ThreeGeometry;
};

const VERTEX_SHADER = `
  varying vec3 vNormal;
  varying vec3 vViewDir;
  varying vec3 vPos;
  varying vec2 vScreenXY;
  uniform float uTime;

  void main() {
    vec3 p = position;
    float breathe = sin(uTime * 0.5 + p.x * 1.8 + p.y * 1.3) * 0.012;
    p += normal * breathe;

    vec4 mv = modelViewMatrix * vec4(p, 1.0);
    vNormal = normalize(normalMatrix * normal);
    vViewDir = normalize(-mv.xyz);
    vPos = p;
    vScreenXY = mv.xy;
    gl_Position = projectionMatrix * mv;
  }
`;

const FRAGMENT_SHADER = `
  precision highp float;
  varying vec3 vNormal;
  varying vec3 vViewDir;
  varying vec3 vPos;
  varying vec2 vScreenXY;
  uniform float uTime;
  uniform vec3 uCamObj;
  uniform sampler2D uTexA;
  uniform sampler2D uTexB;
  uniform sampler2D uTexC;
  uniform sampler2D uTexD;
  uniform sampler2D uTexE;

  vec3 mod289(vec3 x){ return x - floor(x * (1.0/289.0)) * 289.0; }
  vec4 mod289(vec4 x){ return x - floor(x * (1.0/289.0)) * 289.0; }
  vec4 permute(vec4 x){ return mod289(((x*34.0)+1.0)*x); }
  vec4 taylorInvSqrt(vec4 r){ return 1.79284291400159 - 0.85373472095314 * r; }
  float snoise(vec3 v){
    const vec2 C = vec2(1.0/6.0, 1.0/3.0);
    const vec4 D = vec4(0.0, 0.5, 1.0, 2.0);
    vec3 i = floor(v + dot(v, C.yyy));
    vec3 x0 = v - i + dot(i, C.xxx);
    vec3 g = step(x0.yzx, x0.xyz);
    vec3 l = 1.0 - g;
    vec3 i1 = min(g.xyz, l.zxy);
    vec3 i2 = max(g.xyz, l.zxy);
    vec3 x1 = x0 - i1 + C.xxx;
    vec3 x2 = x0 - i2 + C.yyy;
    vec3 x3 = x0 - D.yyy;
    i = mod289(i);
    vec4 p = permute(permute(permute(
      i.z + vec4(0.0, i1.z, i2.z, 1.0))
      + i.y + vec4(0.0, i1.y, i2.y, 1.0))
      + i.x + vec4(0.0, i1.x, i2.x, 1.0));
    float n_ = 0.142857142857;
    vec3 ns = n_ * D.wyz - D.xzx;
    vec4 j = p - 49.0 * floor(p * ns.z * ns.z);
    vec4 x_ = floor(j * ns.z);
    vec4 y_ = floor(j - 7.0 * x_);
    vec4 x = x_ * ns.x + ns.yyyy;
    vec4 y = y_ * ns.x + ns.yyyy;
    vec4 h = 1.0 - abs(x) - abs(y);
    vec4 b0 = vec4(x.xy, y.xy);
    vec4 b1 = vec4(x.zw, y.zw);
    vec4 s0 = floor(b0) * 2.0 + 1.0;
    vec4 s1 = floor(b1) * 2.0 + 1.0;
    vec4 sh = -step(h, vec4(0.0));
    vec4 a0 = b0.xzyw + s0.xzyw * sh.xxyy;
    vec4 a1 = b1.xzyw + s1.xzyw * sh.zzww;
    vec3 p0 = vec3(a0.xy, h.x);
    vec3 p1 = vec3(a0.zw, h.y);
    vec3 p2 = vec3(a1.xy, h.z);
    vec3 p3 = vec3(a1.zw, h.w);
    vec4 norm = taylorInvSqrt(vec4(dot(p0,p0), dot(p1,p1), dot(p2,p2), dot(p3,p3)));
    p0 *= norm.x; p1 *= norm.y; p2 *= norm.z; p3 *= norm.w;
    vec4 m = max(0.6 - vec4(dot(x0,x0), dot(x1,x1), dot(x2,x2), dot(x3,x3)), 0.0);
    m = m * m;
    return 42.0 * dot(m*m, vec4(dot(p0,x0), dot(p1,x1), dot(p2,x2), dot(p3,x3)));
  }

  void imgWeights(out float wA, out float wB, out float wC, out float wD, out float wE) {
    float cycle = uTime * 0.314;
    float step = 6.2831853 / 5.0;
    wA = pow(max(cos(cycle), 0.0), 3.0);
    wB = pow(max(cos(cycle - step), 0.0), 3.0);
    wC = pow(max(cos(cycle - step * 2.0), 0.0), 3.0);
    wD = pow(max(cos(cycle - step * 3.0), 0.0), 3.0);
    wE = pow(max(cos(cycle - step * 4.0), 0.0), 3.0);
    float s = wA + wB + wC + wD + wE + 1e-5;
    wA /= s; wB /= s; wC /= s; wD /= s; wE /= s;
  }

  vec3 sampleBlend(vec2 uv, float wA, float wB, float wC, float wD, float wE) {
    vec3 a = texture2D(uTexA, uv).rgb;
    vec3 b = texture2D(uTexB, uv).rgb;
    vec3 c = texture2D(uTexC, uv).rgb;
    vec3 d = texture2D(uTexD, uv).rgb;
    vec3 e = texture2D(uTexE, uv).rgb;
    return a * wA + b * wB + c * wC + d * wD + e * wE;
  }

  vec3 blurSample(vec2 uv, float r, float wA, float wB, float wC, float wD, float wE) {
    vec3 c = sampleBlend(uv, wA,wB,wC,wD,wE) * 0.36;
    c += sampleBlend(uv + vec2(r, 0.0), wA,wB,wC,wD,wE) * 0.16;
    c += sampleBlend(uv + vec2(-r, 0.0), wA,wB,wC,wD,wE) * 0.16;
    c += sampleBlend(uv + vec2(0.0, r), wA,wB,wC,wD,wE) * 0.16;
    c += sampleBlend(uv + vec2(0.0, -r), wA,wB,wC,wD,wE) * 0.16;
    return c;
  }

  void main() {
    vec3 n = normalize(vNormal);
    float t = uTime * 0.08;

    float ay = uTime * 0.22;
    float ax = sin(uTime * 0.18) * 0.35;
    mat3 rotY = mat3(
      cos(ay), 0.0, -sin(ay),
      0.0, 1.0, 0.0,
      sin(ay), 0.0, cos(ay)
    );
    mat3 rotX = mat3(
      1.0, 0.0, 0.0,
      0.0, cos(ax), -sin(ax),
      0.0, sin(ax), cos(ax)
    );
    mat3 innerRot = rotY * rotX;

    float w1 = snoise(vPos * 1.1 + vec3(t, -t*0.6, t*0.4));
    vec2 warp = vec2(w1, snoise(vPos * 1.5 + vec3(-t, t*0.7, t*0.3))) * 0.012;

    float wA, wB, wC, wD, wE;
    imgWeights(wA, wB, wC, wD, wE);
    float wMax = max(max(max(wA, wB), max(wC, wD)), wE);
    float morph = 1.0 - smoothstep(0.5, 0.95, wMax);

    vec2 base = vScreenXY * 0.55;
    vec2 parallax = (innerRot * n).xy * 0.08;
    vec2 drift = vec2(sin(uTime * 0.08), cos(uTime * 0.10)) * 0.03;
    vec2 uv = base + parallax + drift + 0.5 + warp;

    float r = 0.014 + morph * 0.040;
    vec3 col = blurSample(uv, r, wA,wB,wC,wD,wE);
    col = mix(col, col * vec3(1.03, 1.0, 0.96), 0.5);
    float l = dot(col, vec3(0.299, 0.587, 0.114));
    col = mix(col, vec3(l), 0.08);
    col = col * 0.95 + 0.03;

    vec3 lightDir = normalize(vec3(0.6, 0.9, 0.5));
    float nl = max(dot(n, lightDir), 0.0);
    col *= 0.62 + 0.38 * nl;

    float edgeBand = length(fwidth(n));
    float edgeDark = smoothstep(0.05, 0.25, edgeBand);
    col *= 1.0 - edgeDark * 0.18;

    float grain = snoise(vec3(gl_FragCoord.xy * 0.7, uTime * 4.0)) * 0.05;
    col += grain;

    float ndv = max(dot(n, vViewDir), 0.0);
    float alpha = 0.85 + 0.15 * smoothstep(0.0, 0.15, ndv);

    gl_FragColor = vec4(col, alpha);
  }
`;

const CUBE_TEXTURES = [
  cubeImage1,
  cubeImage2,
  cubeImage3,
  cubeImage4,
  cubeImage5,
];

function importRuntimeModule<T>(path: string): Promise<T> {
  const nativeImport = new Function("path", "return import(path)") as (
    modulePath: string,
  ) => Promise<T>;
  return nativeImport(path);
}

export function CubeWidget(_props: WidgetRenderProps) {
  const { t } = useTranslation("home");
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const host = container;

    let disposed = false;
    let animationFrame = 0;
    let cleanupScene: (() => void) | undefined;

    async function mountCube() {
      const [THREE, { RoundedBoxGeometry }] = await Promise.all([
        importRuntimeModule<ThreeRuntime>(THREE_MODULE_URL),
        importRuntimeModule<RoundedBoxModule>(ROUNDED_BOX_MODULE_URL),
      ]);

      if (disposed) {
        return;
      }

      const renderer = new THREE.WebGLRenderer({
        antialias: true,
        alpha: true,
      });
      renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
      renderer.setClearColor(0xffffff, 0);
      renderer.outputColorSpace = THREE.SRGBColorSpace;
      renderer.toneMapping = THREE.NoToneMapping;
      renderer.domElement.ariaHidden = "true";
      renderer.domElement.className = "block h-full w-full";
      host.appendChild(renderer.domElement);

      const scene = new THREE.Scene();
      const camera = new THREE.OrthographicCamera(
        -ORTHO_SIZE,
        ORTHO_SIZE,
        ORTHO_SIZE,
        -ORTHO_SIZE,
        0.1,
        100,
      );
      camera.position.set(2.2, -1.4, 2.2);
      camera.lookAt(0, 0, 0);

      const loader = new THREE.TextureLoader();
      loader.setCrossOrigin("anonymous");
      const textures = CUBE_TEXTURES.map((url) => {
        const texture = loader.load(url);
        texture.wrapS = THREE.ClampToEdgeWrapping;
        texture.wrapT = THREE.ClampToEdgeWrapping;
        texture.colorSpace = THREE.SRGBColorSpace;
        texture.minFilter = THREE.LinearMipmapLinearFilter;
        texture.magFilter = THREE.LinearFilter;
        return texture;
      });

      const geometry = new RoundedBoxGeometry(1.6, 1.6, 1.6, 10, 0.22);
      const material = new THREE.ShaderMaterial({
        transparent: true,
        depthWrite: false,
        uniforms: {
          uTime: { value: 0 },
          uCamObj: { value: new THREE.Vector3() },
          uTexA: { value: textures[0] },
          uTexB: { value: textures[1] },
          uTexC: { value: textures[2] },
          uTexD: { value: textures[3] },
          uTexE: { value: textures[4] },
        },
        vertexShader: VERTEX_SHADER,
        fragmentShader: FRAGMENT_SHADER,
      });
      const cube = new THREE.Mesh(geometry, material);
      scene.add(cube);

      const clock = new THREE.Clock();
      const invModel = new THREE.Matrix4();
      const camObj = new THREE.Vector3();
      const prefersReducedMotion = window.matchMedia(
        "(prefers-reduced-motion: reduce)",
      ).matches;

      const resize = () => {
        const { width, height } = host.getBoundingClientRect();
        const safeWidth = Math.max(1, Math.round(width));
        const safeHeight = Math.max(1, Math.round(height));
        const aspect = safeWidth / safeHeight;
        camera.left = -ORTHO_SIZE * aspect;
        camera.right = ORTHO_SIZE * aspect;
        camera.top = ORTHO_SIZE;
        camera.bottom = -ORTHO_SIZE;
        camera.updateProjectionMatrix();
        renderer.setSize(safeWidth, safeHeight, false);
      };

      const resizeObserver = new ResizeObserver(resize);
      resizeObserver.observe(host);
      resize();

      let velX = 0;
      let velY = 0;

      const tick = () => {
        if (disposed) {
          return;
        }

        const elapsed = clock.getElapsedTime();
        material.uniforms.uTime.value = prefersReducedMotion ? 0 : elapsed;

        if (!prefersReducedMotion) {
          cube.rotation.y += velX;
          cube.rotation.x += velY;
          velX = velX * 0.975 + 0.002 * 0.025;
          velY = velY * 0.975 + 0.0006 * 0.025;
        }

        cube.updateMatrixWorld();
        invModel.copy(cube.matrixWorld).invert();
        camObj.setFromMatrixPosition(camera.matrixWorld).applyMatrix4(invModel);
        material.uniforms.uCamObj.value.copy(camObj);

        renderer.render(scene, camera);
        animationFrame = window.requestAnimationFrame(tick);
      };

      tick();

      cleanupScene = () => {
        resizeObserver.disconnect();
        window.cancelAnimationFrame(animationFrame);
        renderer.domElement.remove();
        for (const texture of textures) {
          texture.dispose();
        }
        geometry.dispose();
        material.dispose();
        renderer.dispose();
      };
    }

    void mountCube().catch((error) => {
      console.error("Failed to mount home cube:", error);
    });

    return () => {
      disposed = true;
      cleanupScene?.();
    };
  }, []);

  return (
    <section aria-label={t("widgets.cube.ariaLabel")} className="h-full w-full">
      <div ref={containerRef} className="h-full w-full" />
    </section>
  );
}
