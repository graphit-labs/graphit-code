function G(){}
function F(){}
function F(){}
function caller(){G();F();Missing(); const G=()=>0; G(); (()=>F())();}
export class C {value=1; read(){return this.value} write(){this.value=2; this.value+=3; this["value"]=4}}
