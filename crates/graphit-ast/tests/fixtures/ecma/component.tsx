import React from "react";
export interface Props {label:string}
export class View {count=1; render(props:Props){if(this.count&&props.label)return <div>{props.label}</div>;return null}}
export const App=(props:Props)=><View/>;
