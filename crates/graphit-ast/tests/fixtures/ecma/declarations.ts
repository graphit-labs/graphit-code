function mark(x:unknown){return x}
export interface I{read(x?:number):number}
export type T=I;
export enum E{A=1,B}
@mark export class C implements I {value:number=1; read(x?:number){return this.value}}
