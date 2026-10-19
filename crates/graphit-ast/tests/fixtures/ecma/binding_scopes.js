import Default, {outside as Alias} from "pkg";
import * as ns from "pkg";
function rest(){}
function outer(param, {nested: binding}, ...rest) {
    before();
    const before=()=>0;
    before();
    var hoisted=function(){};
    { let Block=()=>0; Block(); function Legacy(){} Legacy(); }
    param(); binding(); rest(); Alias(); ns.run();
}
class C {
    field=1;
    static field=2;
    instance() {
        const lexical=()=>this.field;
        function dynamic(){return this.field;}
        this.field=2;
        this.field+=3;
    }
    static staticMethod(){return this.field;}
}
