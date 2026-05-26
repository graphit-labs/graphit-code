declare module 'd3-force-3d' {
  export function forceX(x?: number): { strength(s: number): any }
  export function forceY(y?: number): { strength(s: number): any }
  export function forceZ(z?: number): { strength(s: number): any }
}
