/**
 * @license
 * Copyright 2010-2026 Three.js Authors
 * SPDX-License-Identifier: MIT
 */const pi={ROTATE:0,DOLLY:1,PAN:2},di={ROTATE:0,PAN:1,DOLLY_PAN:2,DOLLY_ROTATE:3},$l=0,Xa=1,Jl=2,Er=1,Ql=2,Li=3,kn=0,Ue=1,gn=2,vn=0,Ui=1,Ya=2,qa=3,Za=4,jl=5,ui=100,tc=101,ec=102,nc=103,ic=104,rc=200,sc=201,ac=202,oc=203,nl=204,il=205,lc=206,cc=207,hc=208,uc=209,dc=210,fc=211,pc=212,mc=213,_c=214,Ts=0,As=1,ws=2,Fi=3,Rs=4,Cs=5,Ps=6,Ds=7,rl=0,gc=1,xc=2,on=0,sl=1,al=2,ol=3,ll=4,cl=5,hl=6,ul=7,dl=300,Wn=301,xi=302,Wr=303,Xr=304,Or=306,Ls=1e3,xn=1001,Is=1002,Me=1003,vc=1004,Xi=1005,Ae=1006,Yr=1007,Hn=1008,Be=1009,fl=1010,pl=1011,Oi=1012,pa=1013,ln=1014,Ke=1015,cn=1016,ma=1017,_a=1018,Bi=1020,ml=35902,_l=35899,gl=1021,xl=1022,$e=1023,En=1026,Vn=1027,ga=1028,xa=1029,Xn=1030,va=1031,Sa=1033,yr=33776,br=33777,Tr=33778,Ar=33779,Us=35840,Ns=35841,Fs=35842,Os=35843,Bs=36196,zs=37492,Gs=37496,Hs=37488,Vs=37489,Rr=37490,ks=37491,Ws=37808,Xs=37809,Ys=37810,qs=37811,Zs=37812,Ks=37813,$s=37814,Js=37815,Qs=37816,js=37817,ta=37818,ea=37819,na=37820,ia=37821,ra=36492,sa=36494,aa=36495,oa=36283,la=36284,Cr=36285,ca=36286,Sc=3200,ha=0,Mc=1,Dn="",He="srgb",Pr="srgb-linear",Dr="linear",$t="srgb",qr=7680,Ec=519,yc=512,bc=513,Tc=514,Ma=515,Ac=516,wc=517,Ea=518,Rc=519,vl=35044,qm=35048,Ka="300 es",an=2e3,zi=2001;function Cc(i){for(let t=i.length-1;t>=0;--t)if(i[t]>=65535)return!0;return!1}function Lr(i){return document.createElementNS("http://www.w3.org/1999/xhtml",i)}function Pc(){const i=Lr("canvas");return i.style.display="block",i}const $a={};function Ir(...i){const t="THREE."+i.shift();console.log(t,...i)}function Sl(i){const t=i[0];if(typeof t=="string"&&t.startsWith("TSL:")){const e=i[1];e&&e.isStackTrace?i[0]+=" "+e.getLocation():i[1]='Stack trace not available. Enable "THREE.Node.captureStackTrace" to capture stack traces.'}return i}function Rt(...i){i=Sl(i);const t="THREE."+i.shift();{const e=i[0];e&&e.isStackTrace?console.warn(e.getError(t)):console.warn(t,...i)}}function Wt(...i){i=Sl(i);const t="THREE."+i.shift();{const e=i[0];e&&e.isStackTrace?console.error(e.getError(t)):console.error(t,...i)}}function mi(...i){const t=i.join(" ");t in $a||($a[t]=!0,Rt(...i))}function Dc(i,t,e){return new Promise(function(n,r){function s(){switch(i.clientWaitSync(t,i.SYNC_FLUSH_COMMANDS_BIT,0)){case i.WAIT_FAILED:r();break;case i.TIMEOUT_EXPIRED:setTimeout(s,e);break;default:n()}}setTimeout(s,e)})}const Lc={[Ts]:As,[ws]:Ps,[Rs]:Ds,[Fi]:Cs,[As]:Ts,[Ps]:ws,[Ds]:Rs,[Cs]:Fi};class Un{addEventListener(t,e){this._listeners===void 0&&(this._listeners={});const n=this._listeners;n[t]===void 0&&(n[t]=[]),n[t].indexOf(e)===-1&&n[t].push(e)}hasEventListener(t,e){const n=this._listeners;return n===void 0?!1:n[t]!==void 0&&n[t].indexOf(e)!==-1}removeEventListener(t,e){const n=this._listeners;if(n===void 0)return;const r=n[t];if(r!==void 0){const s=r.indexOf(e);s!==-1&&r.splice(s,1)}}dispatchEvent(t){const e=this._listeners;if(e===void 0)return;const n=e[t.type];if(n!==void 0){t.target=this;const r=n.slice(0);for(let s=0,a=r.length;s<a;s++)r[s].call(this,t);t.target=null}}}const be=["00","01","02","03","04","05","06","07","08","09","0a","0b","0c","0d","0e","0f","10","11","12","13","14","15","16","17","18","19","1a","1b","1c","1d","1e","1f","20","21","22","23","24","25","26","27","28","29","2a","2b","2c","2d","2e","2f","30","31","32","33","34","35","36","37","38","39","3a","3b","3c","3d","3e","3f","40","41","42","43","44","45","46","47","48","49","4a","4b","4c","4d","4e","4f","50","51","52","53","54","55","56","57","58","59","5a","5b","5c","5d","5e","5f","60","61","62","63","64","65","66","67","68","69","6a","6b","6c","6d","6e","6f","70","71","72","73","74","75","76","77","78","79","7a","7b","7c","7d","7e","7f","80","81","82","83","84","85","86","87","88","89","8a","8b","8c","8d","8e","8f","90","91","92","93","94","95","96","97","98","99","9a","9b","9c","9d","9e","9f","a0","a1","a2","a3","a4","a5","a6","a7","a8","a9","aa","ab","ac","ad","ae","af","b0","b1","b2","b3","b4","b5","b6","b7","b8","b9","ba","bb","bc","bd","be","bf","c0","c1","c2","c3","c4","c5","c6","c7","c8","c9","ca","cb","cc","cd","ce","cf","d0","d1","d2","d3","d4","d5","d6","d7","d8","d9","da","db","dc","dd","de","df","e0","e1","e2","e3","e4","e5","e6","e7","e8","e9","ea","eb","ec","ed","ee","ef","f0","f1","f2","f3","f4","f5","f6","f7","f8","f9","fa","fb","fc","fd","fe","ff"];let Ja=1234567;const _i=Math.PI/180,Gi=180/Math.PI;function Sn(){const i=Math.random()*4294967295|0,t=Math.random()*4294967295|0,e=Math.random()*4294967295|0,n=Math.random()*4294967295|0;return(be[i&255]+be[i>>8&255]+be[i>>16&255]+be[i>>24&255]+"-"+be[t&255]+be[t>>8&255]+"-"+be[t>>16&15|64]+be[t>>24&255]+"-"+be[e&63|128]+be[e>>8&255]+"-"+be[e>>16&255]+be[e>>24&255]+be[n&255]+be[n>>8&255]+be[n>>16&255]+be[n>>24&255]).toLowerCase()}function Lt(i,t,e){return Math.max(t,Math.min(e,i))}function ya(i,t){return(i%t+t)%t}function Ic(i,t,e,n,r){return n+(i-t)*(r-n)/(e-t)}function Uc(i,t,e){return i!==t?(e-i)/(t-i):0}function Ni(i,t,e){return(1-e)*i+e*t}function Nc(i,t,e,n){return Ni(i,t,1-Math.exp(-e*n))}function Fc(i,t=1){return t-Math.abs(ya(i,t*2)-t)}function Oc(i,t,e){return i<=t?0:i>=e?1:(i=(i-t)/(e-t),i*i*(3-2*i))}function Bc(i,t,e){return i<=t?0:i>=e?1:(i=(i-t)/(e-t),i*i*i*(i*(i*6-15)+10))}function zc(i,t){return i+Math.floor(Math.random()*(t-i+1))}function Gc(i,t){return i+Math.random()*(t-i)}function Hc(i){return i*(.5-Math.random())}function Vc(i){i!==void 0&&(Ja=i);let t=Ja+=1831565813;return t=Math.imul(t^t>>>15,t|1),t^=t+Math.imul(t^t>>>7,t|61),((t^t>>>14)>>>0)/4294967296}function kc(i){return i*_i}function Wc(i){return i*Gi}function Xc(i){return i>0&&Number.isInteger(i)&&2**Math.round(Math.log2(i))===i}function Yc(i){return Math.pow(2,Math.ceil(Math.log(i)/Math.LN2))}function qc(i){return Math.pow(2,Math.floor(Math.log(i)/Math.LN2))}function Zc(i,t,e,n,r){const s=Math.cos,a=Math.sin,o=s(e/2),c=a(e/2),l=s((t+n)/2),d=a((t+n)/2),p=s((t-n)/2),h=a((t-n)/2),_=s((n-t)/2),v=a((n-t)/2);switch(r){case"XYX":i.set(o*d,c*p,c*h,o*l);break;case"YZY":i.set(c*h,o*d,c*p,o*l);break;case"ZXZ":i.set(c*p,c*h,o*d,o*l);break;case"XZX":i.set(o*d,c*v,c*_,o*l);break;case"YXY":i.set(c*_,o*d,c*v,o*l);break;case"ZYZ":i.set(c*v,c*_,o*d,o*l);break;default:Rt("MathUtils: .setQuaternionFromProperEuler() encountered an unknown order: "+r)}}function Ze(i,t){switch(t.constructor){case Float32Array:return i;case Uint32Array:return i/4294967295;case Uint16Array:return i/65535;case Uint8Array:case Uint8ClampedArray:return i/255;case Int32Array:return Math.max(i/2147483647,-1);case Int16Array:return Math.max(i/32767,-1);case Int8Array:return Math.max(i/127,-1);default:throw new Error("THREE.MathUtils: Invalid component type.")}}function Jt(i,t){switch(t.constructor){case Float32Array:return i;case Uint32Array:return Math.round(i*4294967295);case Uint16Array:return Math.round(i*65535);case Uint8Array:case Uint8ClampedArray:return Math.round(i*255);case Int32Array:return Math.round(i*2147483647);case Int16Array:return Math.round(i*32767);case Int8Array:return Math.round(i*127);default:throw new Error("THREE.MathUtils: Invalid component type.")}}const Kc={DEG2RAD:_i,RAD2DEG:Gi,generateUUID:Sn,clamp:Lt,euclideanModulo:ya,mapLinear:Ic,inverseLerp:Uc,lerp:Ni,damp:Nc,pingpong:Fc,smoothstep:Oc,smootherstep:Bc,randInt:zc,randFloat:Gc,randFloatSpread:Hc,seededRandom:Vc,degToRad:kc,radToDeg:Wc,isPowerOfTwo:Xc,ceilPowerOfTwo:Yc,floorPowerOfTwo:qc,setQuaternionFromProperEuler:Zc,normalize:Jt,denormalize:Ze},Pa=class Pa{constructor(t=0,e=0){this.x=t,this.y=e}get width(){return this.x}set width(t){this.x=t}get height(){return this.y}set height(t){this.y=t}set(t,e){return this.x=t,this.y=e,this}setScalar(t){return this.x=t,this.y=t,this}setX(t){return this.x=t,this}setY(t){return this.y=t,this}setComponent(t,e){switch(t){case 0:this.x=e;break;case 1:this.y=e;break;default:throw new Error("THREE.Vector2: index is out of range: "+t)}return this}getComponent(t){switch(t){case 0:return this.x;case 1:return this.y;default:throw new Error("THREE.Vector2: index is out of range: "+t)}}clone(){return new this.constructor(this.x,this.y)}copy(t){return this.x=t.x,this.y=t.y,this}add(t){return this.x+=t.x,this.y+=t.y,this}addScalar(t){return this.x+=t,this.y+=t,this}addVectors(t,e){return this.x=t.x+e.x,this.y=t.y+e.y,this}addScaledVector(t,e){return this.x+=t.x*e,this.y+=t.y*e,this}sub(t){return this.x-=t.x,this.y-=t.y,this}subScalar(t){return this.x-=t,this.y-=t,this}subVectors(t,e){return this.x=t.x-e.x,this.y=t.y-e.y,this}multiply(t){return this.x*=t.x,this.y*=t.y,this}multiplyScalar(t){return this.x*=t,this.y*=t,this}divide(t){return this.x/=t.x,this.y/=t.y,this}divideScalar(t){return this.multiplyScalar(1/t)}applyMatrix3(t){const e=this.x,n=this.y,r=t.elements;return this.x=r[0]*e+r[3]*n+r[6],this.y=r[1]*e+r[4]*n+r[7],this}min(t){return this.x=Math.min(this.x,t.x),this.y=Math.min(this.y,t.y),this}max(t){return this.x=Math.max(this.x,t.x),this.y=Math.max(this.y,t.y),this}clamp(t,e){return this.x=Lt(this.x,t.x,e.x),this.y=Lt(this.y,t.y,e.y),this}clampScalar(t,e){return this.x=Lt(this.x,t,e),this.y=Lt(this.y,t,e),this}clampLength(t,e){const n=this.length();return this.divideScalar(n||1).multiplyScalar(Lt(n,t,e))}floor(){return this.x=Math.floor(this.x),this.y=Math.floor(this.y),this}ceil(){return this.x=Math.ceil(this.x),this.y=Math.ceil(this.y),this}round(){return this.x=Math.round(this.x),this.y=Math.round(this.y),this}roundToZero(){return this.x=Math.trunc(this.x),this.y=Math.trunc(this.y),this}negate(){return this.x=-this.x,this.y=-this.y,this}dot(t){return this.x*t.x+this.y*t.y}cross(t){return this.x*t.y-this.y*t.x}lengthSq(){return this.x*this.x+this.y*this.y}length(){return Math.sqrt(this.x*this.x+this.y*this.y)}manhattanLength(){return Math.abs(this.x)+Math.abs(this.y)}normalize(){return this.divideScalar(this.length()||1)}angle(){return Math.atan2(-this.y,-this.x)+Math.PI}angleTo(t){const e=Math.sqrt(this.lengthSq()*t.lengthSq());if(e===0)return Math.PI/2;const n=this.dot(t)/e;return Math.acos(Lt(n,-1,1))}distanceTo(t){return Math.sqrt(this.distanceToSquared(t))}distanceToSquared(t){const e=this.x-t.x,n=this.y-t.y;return e*e+n*n}manhattanDistanceTo(t){return Math.abs(this.x-t.x)+Math.abs(this.y-t.y)}setLength(t){return this.normalize().multiplyScalar(t)}lerp(t,e){return this.x+=(t.x-this.x)*e,this.y+=(t.y-this.y)*e,this}lerpVectors(t,e,n){return this.x=t.x+(e.x-t.x)*n,this.y=t.y+(e.y-t.y)*n,this}equals(t){return t.x===this.x&&t.y===this.y}fromArray(t,e=0){return this.x=t[e],this.y=t[e+1],this}toArray(t=[],e=0){return t[e]=this.x,t[e+1]=this.y,t}fromBufferAttribute(t,e){return this.x=t.getX(e),this.y=t.getY(e),this}rotateAround(t,e){const n=Math.cos(e),r=Math.sin(e),s=this.x-t.x,a=this.y-t.y;return this.x=s*n-a*r+t.x,this.y=s*r+a*n+t.y,this}random(){return this.x=Math.random(),this.y=Math.random(),this}*[Symbol.iterator](){yield this.x,yield this.y}};Pa.prototype.isVector2=!0;let Ct=Pa;class Ln{constructor(t=0,e=0,n=0,r=1){this.isQuaternion=!0,this._x=t,this._y=e,this._z=n,this._w=r}static slerpFlat(t,e,n,r,s,a,o){let c=n[r+0],l=n[r+1],d=n[r+2],p=n[r+3],h=s[a+0],_=s[a+1],v=s[a+2],M=s[a+3];if(p!==M||c!==h||l!==_||d!==v){let m=c*h+l*_+d*v+p*M;m<0&&(h=-h,_=-_,v=-v,M=-M,m=-m);let u=1-o;if(m<.9995){const A=Math.acos(m),P=Math.sin(A);u=Math.sin(u*A)/P,o=Math.sin(o*A)/P,c=c*u+h*o,l=l*u+_*o,d=d*u+v*o,p=p*u+M*o}else{c=c*u+h*o,l=l*u+_*o,d=d*u+v*o,p=p*u+M*o;const A=1/Math.sqrt(c*c+l*l+d*d+p*p);c*=A,l*=A,d*=A,p*=A}}t[e]=c,t[e+1]=l,t[e+2]=d,t[e+3]=p}static multiplyQuaternionsFlat(t,e,n,r,s,a){const o=n[r],c=n[r+1],l=n[r+2],d=n[r+3],p=s[a],h=s[a+1],_=s[a+2],v=s[a+3];return t[e]=o*v+d*p+c*_-l*h,t[e+1]=c*v+d*h+l*p-o*_,t[e+2]=l*v+d*_+o*h-c*p,t[e+3]=d*v-o*p-c*h-l*_,t}get x(){return this._x}set x(t){this._x=t,this._onChangeCallback()}get y(){return this._y}set y(t){this._y=t,this._onChangeCallback()}get z(){return this._z}set z(t){this._z=t,this._onChangeCallback()}get w(){return this._w}set w(t){this._w=t,this._onChangeCallback()}set(t,e,n,r){return this._x=t,this._y=e,this._z=n,this._w=r,this._onChangeCallback(),this}clone(){return new this.constructor(this._x,this._y,this._z,this._w)}copy(t){return this._x=t.x,this._y=t.y,this._z=t.z,this._w=t.w,this._onChangeCallback(),this}setFromEuler(t,e=!0){const n=t._x,r=t._y,s=t._z,a=t._order,o=Math.cos,c=Math.sin,l=o(n/2),d=o(r/2),p=o(s/2),h=c(n/2),_=c(r/2),v=c(s/2);switch(a){case"XYZ":this._x=h*d*p+l*_*v,this._y=l*_*p-h*d*v,this._z=l*d*v+h*_*p,this._w=l*d*p-h*_*v;break;case"YXZ":this._x=h*d*p+l*_*v,this._y=l*_*p-h*d*v,this._z=l*d*v-h*_*p,this._w=l*d*p+h*_*v;break;case"ZXY":this._x=h*d*p-l*_*v,this._y=l*_*p+h*d*v,this._z=l*d*v+h*_*p,this._w=l*d*p-h*_*v;break;case"ZYX":this._x=h*d*p-l*_*v,this._y=l*_*p+h*d*v,this._z=l*d*v-h*_*p,this._w=l*d*p+h*_*v;break;case"YZX":this._x=h*d*p+l*_*v,this._y=l*_*p+h*d*v,this._z=l*d*v-h*_*p,this._w=l*d*p-h*_*v;break;case"XZY":this._x=h*d*p-l*_*v,this._y=l*_*p-h*d*v,this._z=l*d*v+h*_*p,this._w=l*d*p+h*_*v;break;default:Rt("Quaternion: .setFromEuler() encountered an unknown order: "+a)}return e===!0&&this._onChangeCallback(),this}setFromAxisAngle(t,e){const n=e/2,r=Math.sin(n);return this._x=t.x*r,this._y=t.y*r,this._z=t.z*r,this._w=Math.cos(n),this._onChangeCallback(),this}setFromRotationMatrix(t){const e=t.elements,n=e[0],r=e[4],s=e[8],a=e[1],o=e[5],c=e[9],l=e[2],d=e[6],p=e[10],h=n+o+p;if(h>0){const _=.5/Math.sqrt(h+1);this._w=.25/_,this._x=(d-c)*_,this._y=(s-l)*_,this._z=(a-r)*_}else if(n>o&&n>p){const _=2*Math.sqrt(1+n-o-p);this._w=(d-c)/_,this._x=.25*_,this._y=(r+a)/_,this._z=(s+l)/_}else if(o>p){const _=2*Math.sqrt(1+o-n-p);this._w=(s-l)/_,this._x=(r+a)/_,this._y=.25*_,this._z=(c+d)/_}else{const _=2*Math.sqrt(1+p-n-o);this._w=(a-r)/_,this._x=(s+l)/_,this._y=(c+d)/_,this._z=.25*_}return this._onChangeCallback(),this}setFromUnitVectors(t,e){let n=t.dot(e)+1;return n<1e-8?(n=0,Math.abs(t.x)>Math.abs(t.z)?(this._x=-t.y,this._y=t.x,this._z=0,this._w=n):(this._x=0,this._y=-t.z,this._z=t.y,this._w=n)):(this._x=t.y*e.z-t.z*e.y,this._y=t.z*e.x-t.x*e.z,this._z=t.x*e.y-t.y*e.x,this._w=n),this.normalize()}angleTo(t){return 2*Math.acos(Math.abs(Lt(this.dot(t),-1,1)))}rotateTowards(t,e){const n=this.angleTo(t);if(n===0)return this;const r=Math.min(1,e/n);return this.slerp(t,r),this}identity(){return this.set(0,0,0,1)}invert(){return this.conjugate()}conjugate(){return this._x*=-1,this._y*=-1,this._z*=-1,this._onChangeCallback(),this}dot(t){return this._x*t._x+this._y*t._y+this._z*t._z+this._w*t._w}lengthSq(){return this._x*this._x+this._y*this._y+this._z*this._z+this._w*this._w}length(){return Math.sqrt(this._x*this._x+this._y*this._y+this._z*this._z+this._w*this._w)}normalize(){let t=this.length();return t===0?(this._x=0,this._y=0,this._z=0,this._w=1):(t=1/t,this._x=this._x*t,this._y=this._y*t,this._z=this._z*t,this._w=this._w*t),this._onChangeCallback(),this}multiply(t){return this.multiplyQuaternions(this,t)}premultiply(t){return this.multiplyQuaternions(t,this)}multiplyQuaternions(t,e){const n=t._x,r=t._y,s=t._z,a=t._w,o=e._x,c=e._y,l=e._z,d=e._w;return this._x=n*d+a*o+r*l-s*c,this._y=r*d+a*c+s*o-n*l,this._z=s*d+a*l+n*c-r*o,this._w=a*d-n*o-r*c-s*l,this._onChangeCallback(),this}slerp(t,e){let n=t._x,r=t._y,s=t._z,a=t._w,o=this.dot(t);o<0&&(n=-n,r=-r,s=-s,a=-a,o=-o);let c=1-e;if(o<.9995){const l=Math.acos(o),d=Math.sin(l);c=Math.sin(c*l)/d,e=Math.sin(e*l)/d,this._x=this._x*c+n*e,this._y=this._y*c+r*e,this._z=this._z*c+s*e,this._w=this._w*c+a*e,this._onChangeCallback()}else this._x=this._x*c+n*e,this._y=this._y*c+r*e,this._z=this._z*c+s*e,this._w=this._w*c+a*e,this.normalize();return this}slerpQuaternions(t,e,n){return this.copy(t).slerp(e,n)}random(){const t=2*Math.PI*Math.random(),e=2*Math.PI*Math.random(),n=Math.random(),r=Math.sqrt(1-n),s=Math.sqrt(n);return this.set(r*Math.sin(t),r*Math.cos(t),s*Math.sin(e),s*Math.cos(e))}equals(t){return t._x===this._x&&t._y===this._y&&t._z===this._z&&t._w===this._w}fromArray(t,e=0){return this._x=t[e],this._y=t[e+1],this._z=t[e+2],this._w=t[e+3],this._onChangeCallback(),this}toArray(t=[],e=0){return t[e]=this._x,t[e+1]=this._y,t[e+2]=this._z,t[e+3]=this._w,t}fromBufferAttribute(t,e){return this._x=t.getX(e),this._y=t.getY(e),this._z=t.getZ(e),this._w=t.getW(e),this._onChangeCallback(),this}toJSON(){return this.toArray()}_onChange(t){return this._onChangeCallback=t,this}_onChangeCallback(){}*[Symbol.iterator](){yield this._x,yield this._y,yield this._z,yield this._w}}const Da=class Da{constructor(t=0,e=0,n=0){this.x=t,this.y=e,this.z=n}set(t,e,n){return n===void 0&&(n=this.z),this.x=t,this.y=e,this.z=n,this}setScalar(t){return this.x=t,this.y=t,this.z=t,this}setX(t){return this.x=t,this}setY(t){return this.y=t,this}setZ(t){return this.z=t,this}setComponent(t,e){switch(t){case 0:this.x=e;break;case 1:this.y=e;break;case 2:this.z=e;break;default:throw new Error("THREE.Vector3: index is out of range: "+t)}return this}getComponent(t){switch(t){case 0:return this.x;case 1:return this.y;case 2:return this.z;default:throw new Error("THREE.Vector3: index is out of range: "+t)}}clone(){return new this.constructor(this.x,this.y,this.z)}copy(t){return this.x=t.x,this.y=t.y,this.z=t.z,this}add(t){return this.x+=t.x,this.y+=t.y,this.z+=t.z,this}addScalar(t){return this.x+=t,this.y+=t,this.z+=t,this}addVectors(t,e){return this.x=t.x+e.x,this.y=t.y+e.y,this.z=t.z+e.z,this}addScaledVector(t,e){return this.x+=t.x*e,this.y+=t.y*e,this.z+=t.z*e,this}sub(t){return this.x-=t.x,this.y-=t.y,this.z-=t.z,this}subScalar(t){return this.x-=t,this.y-=t,this.z-=t,this}subVectors(t,e){return this.x=t.x-e.x,this.y=t.y-e.y,this.z=t.z-e.z,this}multiply(t){return this.x*=t.x,this.y*=t.y,this.z*=t.z,this}multiplyScalar(t){return this.x*=t,this.y*=t,this.z*=t,this}multiplyVectors(t,e){return this.x=t.x*e.x,this.y=t.y*e.y,this.z=t.z*e.z,this}applyEuler(t){return this.applyQuaternion(Qa.setFromEuler(t))}applyAxisAngle(t,e){return this.applyQuaternion(Qa.setFromAxisAngle(t,e))}applyMatrix3(t){const e=this.x,n=this.y,r=this.z,s=t.elements;return this.x=s[0]*e+s[3]*n+s[6]*r,this.y=s[1]*e+s[4]*n+s[7]*r,this.z=s[2]*e+s[5]*n+s[8]*r,this}applyNormalMatrix(t){return this.applyMatrix3(t).normalize()}applyMatrix4(t){const e=this.x,n=this.y,r=this.z,s=t.elements,a=1/(s[3]*e+s[7]*n+s[11]*r+s[15]);return this.x=(s[0]*e+s[4]*n+s[8]*r+s[12])*a,this.y=(s[1]*e+s[5]*n+s[9]*r+s[13])*a,this.z=(s[2]*e+s[6]*n+s[10]*r+s[14])*a,this}applyQuaternion(t){const e=this.x,n=this.y,r=this.z,s=t.x,a=t.y,o=t.z,c=t.w,l=2*(a*r-o*n),d=2*(o*e-s*r),p=2*(s*n-a*e);return this.x=e+c*l+a*p-o*d,this.y=n+c*d+o*l-s*p,this.z=r+c*p+s*d-a*l,this}project(t){return this.applyMatrix4(t.matrixWorldInverse).applyMatrix4(t.projectionMatrix)}unproject(t){return this.applyMatrix4(t.projectionMatrixInverse).applyMatrix4(t.matrixWorld)}transformDirection(t){const e=this.x,n=this.y,r=this.z,s=t.elements;return this.x=s[0]*e+s[4]*n+s[8]*r,this.y=s[1]*e+s[5]*n+s[9]*r,this.z=s[2]*e+s[6]*n+s[10]*r,this.normalize()}divide(t){return this.x/=t.x,this.y/=t.y,this.z/=t.z,this}divideScalar(t){return this.multiplyScalar(1/t)}min(t){return this.x=Math.min(this.x,t.x),this.y=Math.min(this.y,t.y),this.z=Math.min(this.z,t.z),this}max(t){return this.x=Math.max(this.x,t.x),this.y=Math.max(this.y,t.y),this.z=Math.max(this.z,t.z),this}clamp(t,e){return this.x=Lt(this.x,t.x,e.x),this.y=Lt(this.y,t.y,e.y),this.z=Lt(this.z,t.z,e.z),this}clampScalar(t,e){return this.x=Lt(this.x,t,e),this.y=Lt(this.y,t,e),this.z=Lt(this.z,t,e),this}clampLength(t,e){const n=this.length();return this.divideScalar(n||1).multiplyScalar(Lt(n,t,e))}floor(){return this.x=Math.floor(this.x),this.y=Math.floor(this.y),this.z=Math.floor(this.z),this}ceil(){return this.x=Math.ceil(this.x),this.y=Math.ceil(this.y),this.z=Math.ceil(this.z),this}round(){return this.x=Math.round(this.x),this.y=Math.round(this.y),this.z=Math.round(this.z),this}roundToZero(){return this.x=Math.trunc(this.x),this.y=Math.trunc(this.y),this.z=Math.trunc(this.z),this}negate(){return this.x=-this.x,this.y=-this.y,this.z=-this.z,this}dot(t){return this.x*t.x+this.y*t.y+this.z*t.z}lengthSq(){return this.x*this.x+this.y*this.y+this.z*this.z}length(){return Math.sqrt(this.x*this.x+this.y*this.y+this.z*this.z)}manhattanLength(){return Math.abs(this.x)+Math.abs(this.y)+Math.abs(this.z)}normalize(){return this.divideScalar(this.length()||1)}setLength(t){return this.normalize().multiplyScalar(t)}lerp(t,e){return this.x+=(t.x-this.x)*e,this.y+=(t.y-this.y)*e,this.z+=(t.z-this.z)*e,this}lerpVectors(t,e,n){return this.x=t.x+(e.x-t.x)*n,this.y=t.y+(e.y-t.y)*n,this.z=t.z+(e.z-t.z)*n,this}cross(t){return this.crossVectors(this,t)}crossVectors(t,e){const n=t.x,r=t.y,s=t.z,a=e.x,o=e.y,c=e.z;return this.x=r*c-s*o,this.y=s*a-n*c,this.z=n*o-r*a,this}projectOnVector(t){const e=t.lengthSq();if(e===0)return this.set(0,0,0);const n=t.dot(this)/e;return this.copy(t).multiplyScalar(n)}projectOnPlane(t){return Zr.copy(this).projectOnVector(t),this.sub(Zr)}reflect(t){return this.sub(Zr.copy(t).multiplyScalar(2*this.dot(t)))}angleTo(t){const e=Math.sqrt(this.lengthSq()*t.lengthSq());if(e===0)return Math.PI/2;const n=this.dot(t)/e;return Math.acos(Lt(n,-1,1))}distanceTo(t){return Math.sqrt(this.distanceToSquared(t))}distanceToSquared(t){const e=this.x-t.x,n=this.y-t.y,r=this.z-t.z;return e*e+n*n+r*r}manhattanDistanceTo(t){return Math.abs(this.x-t.x)+Math.abs(this.y-t.y)+Math.abs(this.z-t.z)}setFromSpherical(t){return this.setFromSphericalCoords(t.radius,t.phi,t.theta)}setFromSphericalCoords(t,e,n){const r=Math.sin(e)*t;return this.x=r*Math.sin(n),this.y=Math.cos(e)*t,this.z=r*Math.cos(n),this}setFromCylindrical(t){return this.setFromCylindricalCoords(t.radius,t.theta,t.y)}setFromCylindricalCoords(t,e,n){return this.x=t*Math.sin(e),this.y=n,this.z=t*Math.cos(e),this}setFromMatrixPosition(t){const e=t.elements;return this.x=e[12],this.y=e[13],this.z=e[14],this}setFromMatrixScale(t){const e=this.setFromMatrixColumn(t,0).length(),n=this.setFromMatrixColumn(t,1).length(),r=this.setFromMatrixColumn(t,2).length();return this.x=e,this.y=n,this.z=r,this}setFromMatrixColumn(t,e){return this.fromArray(t.elements,e*4)}setFromMatrix3Column(t,e){return this.fromArray(t.elements,e*3)}setFromEuler(t){return this.x=t._x,this.y=t._y,this.z=t._z,this}setFromColor(t){return this.x=t.r,this.y=t.g,this.z=t.b,this}equals(t){return t.x===this.x&&t.y===this.y&&t.z===this.z}fromArray(t,e=0){return this.x=t[e],this.y=t[e+1],this.z=t[e+2],this}toArray(t=[],e=0){return t[e]=this.x,t[e+1]=this.y,t[e+2]=this.z,t}fromBufferAttribute(t,e){return this.x=t.getX(e),this.y=t.getY(e),this.z=t.getZ(e),this}random(){return this.x=Math.random(),this.y=Math.random(),this.z=Math.random(),this}randomDirection(){const t=Math.random()*Math.PI*2,e=Math.random()*2-1,n=Math.sqrt(1-e*e);return this.x=n*Math.cos(t),this.y=e,this.z=n*Math.sin(t),this}*[Symbol.iterator](){yield this.x,yield this.y,yield this.z}};Da.prototype.isVector3=!0;let I=Da;const Zr=new I,Qa=new Ln,La=class La{constructor(t,e,n,r,s,a,o,c,l){this.elements=[1,0,0,0,1,0,0,0,1],t!==void 0&&this.set(t,e,n,r,s,a,o,c,l)}set(t,e,n,r,s,a,o,c,l){const d=this.elements;return d[0]=t,d[1]=r,d[2]=o,d[3]=e,d[4]=s,d[5]=c,d[6]=n,d[7]=a,d[8]=l,this}identity(){return this.set(1,0,0,0,1,0,0,0,1),this}copy(t){const e=this.elements,n=t.elements;return e[0]=n[0],e[1]=n[1],e[2]=n[2],e[3]=n[3],e[4]=n[4],e[5]=n[5],e[6]=n[6],e[7]=n[7],e[8]=n[8],this}extractBasis(t,e,n){return t.setFromMatrix3Column(this,0),e.setFromMatrix3Column(this,1),n.setFromMatrix3Column(this,2),this}setFromMatrix4(t){const e=t.elements;return this.set(e[0],e[4],e[8],e[1],e[5],e[9],e[2],e[6],e[10]),this}multiply(t){return this.multiplyMatrices(this,t)}premultiply(t){return this.multiplyMatrices(t,this)}multiplyMatrices(t,e){const n=t.elements,r=e.elements,s=this.elements,a=n[0],o=n[3],c=n[6],l=n[1],d=n[4],p=n[7],h=n[2],_=n[5],v=n[8],M=r[0],m=r[3],u=r[6],A=r[1],P=r[4],S=r[7],b=r[2],y=r[5],w=r[8];return s[0]=a*M+o*A+c*b,s[3]=a*m+o*P+c*y,s[6]=a*u+o*S+c*w,s[1]=l*M+d*A+p*b,s[4]=l*m+d*P+p*y,s[7]=l*u+d*S+p*w,s[2]=h*M+_*A+v*b,s[5]=h*m+_*P+v*y,s[8]=h*u+_*S+v*w,this}multiplyScalar(t){const e=this.elements;return e[0]*=t,e[3]*=t,e[6]*=t,e[1]*=t,e[4]*=t,e[7]*=t,e[2]*=t,e[5]*=t,e[8]*=t,this}determinant(){const t=this.elements,e=t[0],n=t[1],r=t[2],s=t[3],a=t[4],o=t[5],c=t[6],l=t[7],d=t[8];return e*a*d-e*o*l-n*s*d+n*o*c+r*s*l-r*a*c}invert(){const t=this.elements,e=t[0],n=t[1],r=t[2],s=t[3],a=t[4],o=t[5],c=t[6],l=t[7],d=t[8],p=d*a-o*l,h=o*c-d*s,_=l*s-a*c,v=e*p+n*h+r*_;if(v===0)return this.set(0,0,0,0,0,0,0,0,0);const M=1/v;return t[0]=p*M,t[1]=(r*l-d*n)*M,t[2]=(o*n-r*a)*M,t[3]=h*M,t[4]=(d*e-r*c)*M,t[5]=(r*s-o*e)*M,t[6]=_*M,t[7]=(n*c-l*e)*M,t[8]=(a*e-n*s)*M,this}transpose(){let t;const e=this.elements;return t=e[1],e[1]=e[3],e[3]=t,t=e[2],e[2]=e[6],e[6]=t,t=e[5],e[5]=e[7],e[7]=t,this}getNormalMatrix(t){return this.setFromMatrix4(t).invert().transpose()}transposeIntoArray(t){const e=this.elements;return t[0]=e[0],t[1]=e[3],t[2]=e[6],t[3]=e[1],t[4]=e[4],t[5]=e[7],t[6]=e[2],t[7]=e[5],t[8]=e[8],this}setUvTransform(t,e,n,r,s,a,o){const c=Math.cos(s),l=Math.sin(s);return this.set(n*c,n*l,-n*(c*a+l*o)+a+t,-r*l,r*c,-r*(-l*a+c*o)+o+e,0,0,1),this}scale(t,e){return mi("Matrix3: .scale() is deprecated. Use .makeScale() instead."),this.premultiply(Kr.makeScale(t,e)),this}rotate(t){return mi("Matrix3: .rotate() is deprecated. Use .makeRotation() instead."),this.premultiply(Kr.makeRotation(-t)),this}translate(t,e){return mi("Matrix3: .translate() is deprecated. Use .makeTranslation() instead."),this.premultiply(Kr.makeTranslation(t,e)),this}makeTranslation(t,e){return t.isVector2?this.set(1,0,t.x,0,1,t.y,0,0,1):this.set(1,0,t,0,1,e,0,0,1),this}makeRotation(t){const e=Math.cos(t),n=Math.sin(t);return this.set(e,-n,0,n,e,0,0,0,1),this}makeScale(t,e){return this.set(t,0,0,0,e,0,0,0,1),this}equals(t){const e=this.elements,n=t.elements;for(let r=0;r<9;r++)if(e[r]!==n[r])return!1;return!0}fromArray(t,e=0){for(let n=0;n<9;n++)this.elements[n]=t[n+e];return this}toArray(t=[],e=0){const n=this.elements;return t[e]=n[0],t[e+1]=n[1],t[e+2]=n[2],t[e+3]=n[3],t[e+4]=n[4],t[e+5]=n[5],t[e+6]=n[6],t[e+7]=n[7],t[e+8]=n[8],t}clone(){return new this.constructor().fromArray(this.elements)}};La.prototype.isMatrix3=!0;let Dt=La;const Kr=new Dt,ja=new Dt().set(.4123908,.3575843,.1804808,.212639,.7151687,.0721923,.0193308,.1191948,.9505322),to=new Dt().set(3.2409699,-1.5373832,-.4986108,-.9692436,1.8759675,.0415551,.0556301,-.203977,1.0569715);function $c(){const i={enabled:!0,workingColorSpace:Pr,spaces:{},convert:function(r,s,a){return this.enabled===!1||s===a||!s||!a||(this.spaces[s].transfer===$t&&(r.r=Mn(r.r),r.g=Mn(r.g),r.b=Mn(r.b)),this.spaces[s].primaries!==this.spaces[a].primaries&&(r.applyMatrix3(this.spaces[s].toXYZ),r.applyMatrix3(this.spaces[a].fromXYZ)),this.spaces[a].transfer===$t&&(r.r=gi(r.r),r.g=gi(r.g),r.b=gi(r.b))),r},workingToColorSpace:function(r,s){return this.convert(r,this.workingColorSpace,s)},colorSpaceToWorking:function(r,s){return this.convert(r,s,this.workingColorSpace)},getPrimaries:function(r){return this.spaces[r].primaries},getTransfer:function(r){return r===Dn?Dr:this.spaces[r].transfer},getToneMappingMode:function(r){return this.spaces[r].outputColorSpaceConfig.toneMappingMode||"standard"},getLuminanceCoefficients:function(r,s=this.workingColorSpace){return r.fromArray(this.spaces[s].luminanceCoefficients)},define:function(r){Object.assign(this.spaces,r)},_getMatrix:function(r,s,a){return r.copy(this.spaces[s].toXYZ).multiply(this.spaces[a].fromXYZ)},_getDrawingBufferColorSpace:function(r){return this.spaces[r].outputColorSpaceConfig.drawingBufferColorSpace},_getUnpackColorSpace:function(r=this.workingColorSpace){return this.spaces[r].workingColorSpaceConfig.unpackColorSpace},fromWorkingColorSpace:function(r,s){return mi("ColorManagement: .fromWorkingColorSpace() has been renamed to .workingToColorSpace()."),i.workingToColorSpace(r,s)},toWorkingColorSpace:function(r,s){return mi("ColorManagement: .toWorkingColorSpace() has been renamed to .colorSpaceToWorking()."),i.colorSpaceToWorking(r,s)}},t=[.64,.33,.3,.6,.15,.06],e=[.2126,.7152,.0722],n=[.3127,.329];return i.define({[Pr]:{primaries:t,whitePoint:n,transfer:Dr,toXYZ:ja,fromXYZ:to,luminanceCoefficients:e,workingColorSpaceConfig:{unpackColorSpace:He},outputColorSpaceConfig:{drawingBufferColorSpace:He}},[He]:{primaries:t,whitePoint:n,transfer:$t,toXYZ:ja,fromXYZ:to,luminanceCoefficients:e,outputColorSpaceConfig:{drawingBufferColorSpace:He}}}),i}const Ht=$c();function Mn(i){return i<.04045?i*.0773993808:Math.pow(i*.9478672986+.0521327014,2.4)}function gi(i){return i<.0031308?i*12.92:1.055*Math.pow(i,.41666)-.055}let Kn;class Jc{static getDataURL(t,e="image/png"){if(/^data:/i.test(t.src)||typeof HTMLCanvasElement>"u")return t.src;let n;if(t instanceof HTMLCanvasElement)n=t;else{Kn===void 0&&(Kn=Lr("canvas")),Kn.width=t.width,Kn.height=t.height;const r=Kn.getContext("2d");t instanceof ImageData?r.putImageData(t,0,0):r.drawImage(t,0,0,t.width,t.height),n=Kn}return n.toDataURL(e)}static sRGBToLinear(t){if(typeof HTMLImageElement<"u"&&t instanceof HTMLImageElement||typeof HTMLCanvasElement<"u"&&t instanceof HTMLCanvasElement||typeof ImageBitmap<"u"&&t instanceof ImageBitmap){const e=Lr("canvas");e.width=t.width,e.height=t.height;const n=e.getContext("2d");n.drawImage(t,0,0,t.width,t.height);const r=n.getImageData(0,0,t.width,t.height),s=r.data;for(let a=0;a<s.length;a++)s[a]=Mn(s[a]/255)*255;return n.putImageData(r,0,0),e}else if(t.data){const e=t.data.slice(0);for(let n=0;n<e.length;n++)e instanceof Uint8Array||e instanceof Uint8ClampedArray?e[n]=Math.floor(Mn(e[n]/255)*255):e[n]=Mn(e[n]);return{data:e,width:t.width,height:t.height}}else return Rt("ImageUtils.sRGBToLinear(): Unsupported image type. No color space conversion applied."),t}}let Qc=0;class ba{constructor(t=null){this.isTextureSource=!0,Object.defineProperty(this,"id",{value:Qc++}),this.uuid=Sn(),this.data=t,this.dataReady=!0,this.version=0}getSize(t){const e=this.data;return typeof HTMLVideoElement<"u"&&e instanceof HTMLVideoElement?t.set(e.videoWidth,e.videoHeight,0):typeof VideoFrame<"u"&&e instanceof VideoFrame?t.set(e.displayWidth,e.displayHeight,0):e!==null?t.set(e.width,e.height,e.depth||0):t.set(0,0,0),t}set needsUpdate(t){t===!0&&this.version++}toJSON(t){const e=t===void 0||typeof t=="string";if(!e&&t.images[this.uuid]!==void 0)return t.images[this.uuid];const n={uuid:this.uuid,url:""},r=this.data;if(r!==null){let s;if(Array.isArray(r)){s=[];for(let a=0,o=r.length;a<o;a++)r[a].isDataTexture?s.push($r(r[a].image)):s.push($r(r[a]))}else s=$r(r);n.url=s}return e||(t.images[this.uuid]=n),n}}function $r(i){return typeof HTMLImageElement<"u"&&i instanceof HTMLImageElement||typeof HTMLCanvasElement<"u"&&i instanceof HTMLCanvasElement||typeof ImageBitmap<"u"&&i instanceof ImageBitmap?Jc.getDataURL(i):i.data?{data:Array.from(i.data),width:i.width,height:i.height,type:i.data.constructor.name}:(Rt("Texture: Unable to serialize Texture."),{})}let jc=0;const Jr=new I;class De extends Un{constructor(t=De.DEFAULT_IMAGE,e=De.DEFAULT_MAPPING,n=xn,r=xn,s=Ae,a=Hn,o=$e,c=Be,l=De.DEFAULT_ANISOTROPY,d=Dn){super(),this.isTexture=!0,Object.defineProperty(this,"id",{value:jc++}),this.uuid=Sn(),this.name="",this.source=new ba(t),this.mipmaps=[],this.mapping=e,this.channel=0,this.wrapS=n,this.wrapT=r,this.magFilter=s,this.minFilter=a,this.anisotropy=l,this.format=o,this.internalFormat=null,this.type=c,this.offset=new Ct(0,0),this.repeat=new Ct(1,1),this.center=new Ct(0,0),this.rotation=0,this.matrixAutoUpdate=!0,this.matrix=new Dt,this.generateMipmaps=!0,this.premultiplyAlpha=!1,this.flipY=!0,this.unpackAlignment=4,this.colorSpace=d,this.userData={},this.updateRanges=[],this.version=0,this.onUpdate=null,this.renderTarget=null,this.isRenderTargetTexture=!1,this.isArrayTexture=!!(t&&t.depth&&t.depth>1),this.pmremVersion=0,this.normalized=!1}get width(){return this.source.getSize(Jr).x}get height(){return this.source.getSize(Jr).y}get depth(){return this.source.getSize(Jr).z}get image(){return this.source.data}set image(t){this.source.data=t}updateMatrix(){this.matrix.setUvTransform(this.offset.x,this.offset.y,this.repeat.x,this.repeat.y,this.rotation,this.center.x,this.center.y)}addUpdateRange(t,e){this.updateRanges.push({start:t,count:e})}clearUpdateRanges(){this.updateRanges.length=0}clone(){return new this.constructor().copy(this)}copy(t){return this.name=t.name,this.source=t.source,this.mipmaps=t.mipmaps.slice(0),this.mapping=t.mapping,this.channel=t.channel,this.wrapS=t.wrapS,this.wrapT=t.wrapT,this.magFilter=t.magFilter,this.minFilter=t.minFilter,this.anisotropy=t.anisotropy,this.format=t.format,this.internalFormat=t.internalFormat,this.type=t.type,this.normalized=t.normalized,this.offset.copy(t.offset),this.repeat.copy(t.repeat),this.center.copy(t.center),this.rotation=t.rotation,this.matrixAutoUpdate=t.matrixAutoUpdate,this.matrix.copy(t.matrix),this.generateMipmaps=t.generateMipmaps,this.premultiplyAlpha=t.premultiplyAlpha,this.flipY=t.flipY,this.unpackAlignment=t.unpackAlignment,this.colorSpace=t.colorSpace,this.renderTarget=t.renderTarget,this.isRenderTargetTexture=t.isRenderTargetTexture,this.isArrayTexture=t.isArrayTexture,this.userData=JSON.parse(JSON.stringify(t.userData)),this.needsUpdate=!0,this}setValues(t){for(const e in t){const n=t[e];if(n===void 0){Rt(`Texture.setValues(): parameter '${e}' has value of undefined.`);continue}const r=this[e];if(r===void 0){Rt(`Texture.setValues(): property '${e}' does not exist.`);continue}r&&n&&r.isVector2&&n.isVector2||r&&n&&r.isVector3&&n.isVector3||r&&n&&r.isMatrix3&&n.isMatrix3?r.copy(n):this[e]=n}}toJSON(t){const e=t===void 0||typeof t=="string";if(!e&&t.textures[this.uuid]!==void 0)return t.textures[this.uuid];const n={metadata:{version:4.7,type:"Texture",generator:"Texture.toJSON"},uuid:this.uuid,name:this.name,image:this.source.toJSON(t).uuid,mapping:this.mapping,channel:this.channel,repeat:[this.repeat.x,this.repeat.y],offset:[this.offset.x,this.offset.y],center:[this.center.x,this.center.y],rotation:this.rotation,wrap:[this.wrapS,this.wrapT],format:this.format,internalFormat:this.internalFormat,type:this.type,normalized:this.normalized,colorSpace:this.colorSpace,minFilter:this.minFilter,magFilter:this.magFilter,anisotropy:this.anisotropy,flipY:this.flipY,generateMipmaps:this.generateMipmaps,premultiplyAlpha:this.premultiplyAlpha,unpackAlignment:this.unpackAlignment};return Object.keys(this.userData).length>0&&(n.userData=this.userData),e||(t.textures[this.uuid]=n),n}dispose(){this.dispatchEvent({type:"dispose"})}transformUv(t){if(this.mapping!==dl)return t;if(t.applyMatrix3(this.matrix),t.x<0||t.x>1)switch(this.wrapS){case Ls:t.x=t.x-Math.floor(t.x);break;case xn:t.x=t.x<0?0:1;break;case Is:Math.abs(Math.floor(t.x)%2)===1?t.x=Math.ceil(t.x)-t.x:t.x=t.x-Math.floor(t.x);break}if(t.y<0||t.y>1)switch(this.wrapT){case Ls:t.y=t.y-Math.floor(t.y);break;case xn:t.y=t.y<0?0:1;break;case Is:Math.abs(Math.floor(t.y)%2)===1?t.y=Math.ceil(t.y)-t.y:t.y=t.y-Math.floor(t.y);break}return this.flipY&&(t.y=1-t.y),t}set needsUpdate(t){t===!0&&(this.version++,this.source.needsUpdate=!0)}set needsPMREMUpdate(t){t===!0&&this.pmremVersion++}}De.DEFAULT_IMAGE=null;De.DEFAULT_MAPPING=dl;De.DEFAULT_ANISOTROPY=1;const Ia=class Ia{constructor(t=0,e=0,n=0,r=1){this.x=t,this.y=e,this.z=n,this.w=r}get width(){return this.z}set width(t){this.z=t}get height(){return this.w}set height(t){this.w=t}set(t,e,n,r){return this.x=t,this.y=e,this.z=n,this.w=r,this}setScalar(t){return this.x=t,this.y=t,this.z=t,this.w=t,this}setX(t){return this.x=t,this}setY(t){return this.y=t,this}setZ(t){return this.z=t,this}setW(t){return this.w=t,this}setComponent(t,e){switch(t){case 0:this.x=e;break;case 1:this.y=e;break;case 2:this.z=e;break;case 3:this.w=e;break;default:throw new Error("THREE.Vector4: index is out of range: "+t)}return this}getComponent(t){switch(t){case 0:return this.x;case 1:return this.y;case 2:return this.z;case 3:return this.w;default:throw new Error("THREE.Vector4: index is out of range: "+t)}}clone(){return new this.constructor(this.x,this.y,this.z,this.w)}copy(t){return this.x=t.x,this.y=t.y,this.z=t.z,this.w=t.w!==void 0?t.w:1,this}add(t){return this.x+=t.x,this.y+=t.y,this.z+=t.z,this.w+=t.w,this}addScalar(t){return this.x+=t,this.y+=t,this.z+=t,this.w+=t,this}addVectors(t,e){return this.x=t.x+e.x,this.y=t.y+e.y,this.z=t.z+e.z,this.w=t.w+e.w,this}addScaledVector(t,e){return this.x+=t.x*e,this.y+=t.y*e,this.z+=t.z*e,this.w+=t.w*e,this}sub(t){return this.x-=t.x,this.y-=t.y,this.z-=t.z,this.w-=t.w,this}subScalar(t){return this.x-=t,this.y-=t,this.z-=t,this.w-=t,this}subVectors(t,e){return this.x=t.x-e.x,this.y=t.y-e.y,this.z=t.z-e.z,this.w=t.w-e.w,this}multiply(t){return this.x*=t.x,this.y*=t.y,this.z*=t.z,this.w*=t.w,this}multiplyScalar(t){return this.x*=t,this.y*=t,this.z*=t,this.w*=t,this}applyMatrix4(t){const e=this.x,n=this.y,r=this.z,s=this.w,a=t.elements;return this.x=a[0]*e+a[4]*n+a[8]*r+a[12]*s,this.y=a[1]*e+a[5]*n+a[9]*r+a[13]*s,this.z=a[2]*e+a[6]*n+a[10]*r+a[14]*s,this.w=a[3]*e+a[7]*n+a[11]*r+a[15]*s,this}divide(t){return this.x/=t.x,this.y/=t.y,this.z/=t.z,this.w/=t.w,this}divideScalar(t){return this.multiplyScalar(1/t)}setAxisAngleFromQuaternion(t){this.w=2*Math.acos(t.w);const e=Math.sqrt(1-t.w*t.w);return e<1e-4?(this.x=1,this.y=0,this.z=0):(this.x=t.x/e,this.y=t.y/e,this.z=t.z/e),this}setAxisAngleFromRotationMatrix(t){let e,n,r,s;const c=t.elements,l=c[0],d=c[4],p=c[8],h=c[1],_=c[5],v=c[9],M=c[2],m=c[6],u=c[10];if(Math.abs(d-h)<.01&&Math.abs(p-M)<.01&&Math.abs(v-m)<.01){if(Math.abs(d+h)<.1&&Math.abs(p+M)<.1&&Math.abs(v+m)<.1&&Math.abs(l+_+u-3)<.1)return this.set(1,0,0,0),this;e=Math.PI;const P=(l+1)/2,S=(_+1)/2,b=(u+1)/2,y=(d+h)/4,w=(p+M)/4,x=(v+m)/4;return P>S&&P>b?P<.01?(n=0,r=.707106781,s=.707106781):(n=Math.sqrt(P),r=y/n,s=w/n):S>b?S<.01?(n=.707106781,r=0,s=.707106781):(r=Math.sqrt(S),n=y/r,s=x/r):b<.01?(n=.707106781,r=.707106781,s=0):(s=Math.sqrt(b),n=w/s,r=x/s),this.set(n,r,s,e),this}let A=Math.sqrt((m-v)*(m-v)+(p-M)*(p-M)+(h-d)*(h-d));return Math.abs(A)<.001&&(A=1),this.x=(m-v)/A,this.y=(p-M)/A,this.z=(h-d)/A,this.w=Math.acos((l+_+u-1)/2),this}setFromMatrixPosition(t){const e=t.elements;return this.x=e[12],this.y=e[13],this.z=e[14],this.w=e[15],this}min(t){return this.x=Math.min(this.x,t.x),this.y=Math.min(this.y,t.y),this.z=Math.min(this.z,t.z),this.w=Math.min(this.w,t.w),this}max(t){return this.x=Math.max(this.x,t.x),this.y=Math.max(this.y,t.y),this.z=Math.max(this.z,t.z),this.w=Math.max(this.w,t.w),this}clamp(t,e){return this.x=Lt(this.x,t.x,e.x),this.y=Lt(this.y,t.y,e.y),this.z=Lt(this.z,t.z,e.z),this.w=Lt(this.w,t.w,e.w),this}clampScalar(t,e){return this.x=Lt(this.x,t,e),this.y=Lt(this.y,t,e),this.z=Lt(this.z,t,e),this.w=Lt(this.w,t,e),this}clampLength(t,e){const n=this.length();return this.divideScalar(n||1).multiplyScalar(Lt(n,t,e))}floor(){return this.x=Math.floor(this.x),this.y=Math.floor(this.y),this.z=Math.floor(this.z),this.w=Math.floor(this.w),this}ceil(){return this.x=Math.ceil(this.x),this.y=Math.ceil(this.y),this.z=Math.ceil(this.z),this.w=Math.ceil(this.w),this}round(){return this.x=Math.round(this.x),this.y=Math.round(this.y),this.z=Math.round(this.z),this.w=Math.round(this.w),this}roundToZero(){return this.x=Math.trunc(this.x),this.y=Math.trunc(this.y),this.z=Math.trunc(this.z),this.w=Math.trunc(this.w),this}negate(){return this.x=-this.x,this.y=-this.y,this.z=-this.z,this.w=-this.w,this}dot(t){return this.x*t.x+this.y*t.y+this.z*t.z+this.w*t.w}lengthSq(){return this.x*this.x+this.y*this.y+this.z*this.z+this.w*this.w}length(){return Math.sqrt(this.x*this.x+this.y*this.y+this.z*this.z+this.w*this.w)}manhattanLength(){return Math.abs(this.x)+Math.abs(this.y)+Math.abs(this.z)+Math.abs(this.w)}normalize(){return this.divideScalar(this.length()||1)}setLength(t){return this.normalize().multiplyScalar(t)}lerp(t,e){return this.x+=(t.x-this.x)*e,this.y+=(t.y-this.y)*e,this.z+=(t.z-this.z)*e,this.w+=(t.w-this.w)*e,this}lerpVectors(t,e,n){return this.x=t.x+(e.x-t.x)*n,this.y=t.y+(e.y-t.y)*n,this.z=t.z+(e.z-t.z)*n,this.w=t.w+(e.w-t.w)*n,this}equals(t){return t.x===this.x&&t.y===this.y&&t.z===this.z&&t.w===this.w}fromArray(t,e=0){return this.x=t[e],this.y=t[e+1],this.z=t[e+2],this.w=t[e+3],this}toArray(t=[],e=0){return t[e]=this.x,t[e+1]=this.y,t[e+2]=this.z,t[e+3]=this.w,t}fromBufferAttribute(t,e){return this.x=t.getX(e),this.y=t.getY(e),this.z=t.getZ(e),this.w=t.getW(e),this}random(){return this.x=Math.random(),this.y=Math.random(),this.z=Math.random(),this.w=Math.random(),this}*[Symbol.iterator](){yield this.x,yield this.y,yield this.z,yield this.w}};Ia.prototype.isVector4=!0;let ce=Ia;class th extends Un{constructor(t=1,e=1,n={}){super(),n=Object.assign({generateMipmaps:!1,internalFormat:null,minFilter:Ae,depthBuffer:!0,stencilBuffer:!1,resolveColorBuffer:!0,resolveDepthBuffer:!0,resolveStencilBuffer:!0,storeMultisampledColorBuffer:!0,storeMultisampledDepthBuffer:!0,storeMultisampledStencilBuffer:!0,depthTexture:null,samples:0,count:1,depth:1,multiview:!1,useArrayDepthTexture:!1},n),this.isRenderTarget=!0,this.width=t,this.height=e,this.depth=n.depth,this.scissor=new ce(0,0,t,e),this.scissorTest=!1,this.viewport=new ce(0,0,t,e),this.textures=[];const r={width:t,height:e,depth:n.depth},s=new De(r),a=n.count;for(let o=0;o<a;o++)this.textures[o]=s.clone(),this.textures[o].isRenderTargetTexture=!0,this.textures[o].renderTarget=this;this._setTextureOptions(n),this.depthBuffer=n.depthBuffer,this.stencilBuffer=n.stencilBuffer,this.resolveColorBuffer=n.resolveColorBuffer,this.resolveDepthBuffer=n.resolveDepthBuffer,this.resolveStencilBuffer=n.resolveStencilBuffer,this.storeMultisampledColorBuffer=n.storeMultisampledColorBuffer,this.storeMultisampledDepthBuffer=n.storeMultisampledDepthBuffer,this.storeMultisampledStencilBuffer=n.storeMultisampledStencilBuffer,this._depthTexture=null,this.depthTexture=n.depthTexture,this.samples=n.samples,this.multiview=n.multiview,this.useArrayDepthTexture=n.useArrayDepthTexture}_setTextureOptions(t={}){const e={minFilter:Ae,generateMipmaps:!1,flipY:!1,internalFormat:null};t.mapping!==void 0&&(e.mapping=t.mapping),t.wrapS!==void 0&&(e.wrapS=t.wrapS),t.wrapT!==void 0&&(e.wrapT=t.wrapT),t.wrapR!==void 0&&(e.wrapR=t.wrapR),t.magFilter!==void 0&&(e.magFilter=t.magFilter),t.minFilter!==void 0&&(e.minFilter=t.minFilter),t.format!==void 0&&(e.format=t.format),t.type!==void 0&&(e.type=t.type),t.anisotropy!==void 0&&(e.anisotropy=t.anisotropy),t.colorSpace!==void 0&&(e.colorSpace=t.colorSpace),t.flipY!==void 0&&(e.flipY=t.flipY),t.generateMipmaps!==void 0&&(e.generateMipmaps=t.generateMipmaps),t.internalFormat!==void 0&&(e.internalFormat=t.internalFormat);for(let n=0;n<this.textures.length;n++)this.textures[n].setValues(e)}get texture(){return this.textures[0]}set texture(t){this.textures[0]=t}set depthTexture(t){this._depthTexture!==null&&this._depthTexture.renderTarget===this&&(this._depthTexture.renderTarget=null),t!==null&&t.renderTarget===null&&(t.renderTarget=this),this._depthTexture=t}get depthTexture(){return this._depthTexture}setSize(t,e,n=1){if(this.width!==t||this.height!==e||this.depth!==n){this.width=t,this.height=e,this.depth=n;for(let r=0,s=this.textures.length;r<s;r++)this.textures[r].image.width=t,this.textures[r].image.height=e,this.textures[r].image.depth=n,this.textures[r].isData3DTexture!==!0&&(this.textures[r].isArrayTexture=this.textures[r].image.depth>1);this.dispose()}this.viewport.set(0,0,t,e),this.scissor.set(0,0,t,e)}clone(){return new this.constructor().copy(this)}copy(t){this.width=t.width,this.height=t.height,this.depth=t.depth,this.scissor.copy(t.scissor),this.scissorTest=t.scissorTest,this.viewport.copy(t.viewport),this.textures.length=0;for(let e=0,n=t.textures.length;e<n;e++){this.textures[e]=t.textures[e].clone(),this.textures[e].isRenderTargetTexture=!0,this.textures[e].renderTarget=this;const r=Object.assign({},t.textures[e].image);this.textures[e].source=new ba(r)}if(this.depthBuffer=t.depthBuffer,this.stencilBuffer=t.stencilBuffer,this.resolveColorBuffer=t.resolveColorBuffer,this.resolveDepthBuffer=t.resolveDepthBuffer,this.resolveStencilBuffer=t.resolveStencilBuffer,this.storeMultisampledColorBuffer=t.storeMultisampledColorBuffer,this.storeMultisampledDepthBuffer=t.storeMultisampledDepthBuffer,this.storeMultisampledStencilBuffer=t.storeMultisampledStencilBuffer,t.depthTexture!==null)if(t.depthTexture.renderTarget===t){const e=t.depthTexture.clone();e.renderTarget=null,this.depthTexture=e}else this.depthTexture=t.depthTexture;return this.samples=t.samples,this.multiview=t.multiview,this.useArrayDepthTexture=t.useArrayDepthTexture,this}dispose(){this.dispatchEvent({type:"dispose"})}}class Je extends th{constructor(t=1,e=1,n={}){super(t,e,n),this.isWebGLRenderTarget=!0}}class Ml extends De{constructor(t=null,e=1,n=1,r=1){super(null),this.isDataArrayTexture=!0,this.image={data:t,width:e,height:n,depth:r},this.magFilter=Me,this.minFilter=Me,this.wrapR=xn,this.generateMipmaps=!1,this.flipY=!1,this.unpackAlignment=1,this.layerUpdates=new Set}copy(t){return super.copy(t),this.wrapR=t.wrapR,this}addLayerUpdate(t){this.layerUpdates.add(t)}clearLayerUpdates(){this.layerUpdates.clear()}}class eh extends De{constructor(t=null,e=1,n=1,r=1){super(null),this.isData3DTexture=!0,this.image={data:t,width:e,height:n,depth:r},this.magFilter=Me,this.minFilter=Me,this.wrapR=xn,this.generateMipmaps=!1,this.flipY=!1,this.unpackAlignment=1}copy(t){return super.copy(t),this.wrapR=t.wrapR,this}}const Fr=class Fr{constructor(t,e,n,r,s,a,o,c,l,d,p,h,_,v,M,m){this.elements=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],t!==void 0&&this.set(t,e,n,r,s,a,o,c,l,d,p,h,_,v,M,m)}set(t,e,n,r,s,a,o,c,l,d,p,h,_,v,M,m){const u=this.elements;return u[0]=t,u[4]=e,u[8]=n,u[12]=r,u[1]=s,u[5]=a,u[9]=o,u[13]=c,u[2]=l,u[6]=d,u[10]=p,u[14]=h,u[3]=_,u[7]=v,u[11]=M,u[15]=m,this}identity(){return this.set(1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1),this}clone(){return new Fr().fromArray(this.elements)}copy(t){const e=this.elements,n=t.elements;return e[0]=n[0],e[1]=n[1],e[2]=n[2],e[3]=n[3],e[4]=n[4],e[5]=n[5],e[6]=n[6],e[7]=n[7],e[8]=n[8],e[9]=n[9],e[10]=n[10],e[11]=n[11],e[12]=n[12],e[13]=n[13],e[14]=n[14],e[15]=n[15],this}copyPosition(t){const e=this.elements,n=t.elements;return e[12]=n[12],e[13]=n[13],e[14]=n[14],this}setFromMatrix3(t){const e=t.elements;return this.set(e[0],e[3],e[6],0,e[1],e[4],e[7],0,e[2],e[5],e[8],0,0,0,0,1),this}extractBasis(t,e,n){return this.determinantAffine()===0?(t.set(1,0,0),e.set(0,1,0),n.set(0,0,1),this):(t.setFromMatrixColumn(this,0),e.setFromMatrixColumn(this,1),n.setFromMatrixColumn(this,2),this)}makeBasis(t,e,n){return this.set(t.x,e.x,n.x,0,t.y,e.y,n.y,0,t.z,e.z,n.z,0,0,0,0,1),this}extractRotation(t){if(t.determinantAffine()===0)return this.identity();const e=this.elements,n=t.elements,r=1/$n.setFromMatrixColumn(t,0).length(),s=1/$n.setFromMatrixColumn(t,1).length(),a=1/$n.setFromMatrixColumn(t,2).length();return e[0]=n[0]*r,e[1]=n[1]*r,e[2]=n[2]*r,e[3]=0,e[4]=n[4]*s,e[5]=n[5]*s,e[6]=n[6]*s,e[7]=0,e[8]=n[8]*a,e[9]=n[9]*a,e[10]=n[10]*a,e[11]=0,e[12]=0,e[13]=0,e[14]=0,e[15]=1,this}makeRotationFromEuler(t){const e=this.elements,n=t.x,r=t.y,s=t.z,a=Math.cos(n),o=Math.sin(n),c=Math.cos(r),l=Math.sin(r),d=Math.cos(s),p=Math.sin(s);if(t.order==="XYZ"){const h=a*d,_=a*p,v=o*d,M=o*p;e[0]=c*d,e[4]=-c*p,e[8]=l,e[1]=_+v*l,e[5]=h-M*l,e[9]=-o*c,e[2]=M-h*l,e[6]=v+_*l,e[10]=a*c}else if(t.order==="YXZ"){const h=c*d,_=c*p,v=l*d,M=l*p;e[0]=h+M*o,e[4]=v*o-_,e[8]=a*l,e[1]=a*p,e[5]=a*d,e[9]=-o,e[2]=_*o-v,e[6]=M+h*o,e[10]=a*c}else if(t.order==="ZXY"){const h=c*d,_=c*p,v=l*d,M=l*p;e[0]=h-M*o,e[4]=-a*p,e[8]=v+_*o,e[1]=_+v*o,e[5]=a*d,e[9]=M-h*o,e[2]=-a*l,e[6]=o,e[10]=a*c}else if(t.order==="ZYX"){const h=a*d,_=a*p,v=o*d,M=o*p;e[0]=c*d,e[4]=v*l-_,e[8]=h*l+M,e[1]=c*p,e[5]=M*l+h,e[9]=_*l-v,e[2]=-l,e[6]=o*c,e[10]=a*c}else if(t.order==="YZX"){const h=a*c,_=a*l,v=o*c,M=o*l;e[0]=c*d,e[4]=M-h*p,e[8]=v*p+_,e[1]=p,e[5]=a*d,e[9]=-o*d,e[2]=-l*d,e[6]=_*p+v,e[10]=h-M*p}else if(t.order==="XZY"){const h=a*c,_=a*l,v=o*c,M=o*l;e[0]=c*d,e[4]=-p,e[8]=l*d,e[1]=h*p+M,e[5]=a*d,e[9]=_*p-v,e[2]=v*p-_,e[6]=o*d,e[10]=M*p+h}return e[3]=0,e[7]=0,e[11]=0,e[12]=0,e[13]=0,e[14]=0,e[15]=1,this}makeRotationFromQuaternion(t){return this.compose(nh,t,ih)}lookAt(t,e,n){const r=this.elements;return Fe.subVectors(t,e),Fe.lengthSq()===0&&(Fe.z=1),Fe.normalize(),An.crossVectors(n,Fe),An.lengthSq()===0&&(Math.abs(n.z)===1?Fe.x+=1e-4:Fe.z+=1e-4,Fe.normalize(),An.crossVectors(n,Fe)),An.normalize(),Yi.crossVectors(Fe,An),r[0]=An.x,r[4]=Yi.x,r[8]=Fe.x,r[1]=An.y,r[5]=Yi.y,r[9]=Fe.y,r[2]=An.z,r[6]=Yi.z,r[10]=Fe.z,this}multiply(t){return this.multiplyMatrices(this,t)}premultiply(t){return this.multiplyMatrices(t,this)}multiplyMatrices(t,e){const n=t.elements,r=e.elements,s=this.elements,a=n[0],o=n[4],c=n[8],l=n[12],d=n[1],p=n[5],h=n[9],_=n[13],v=n[2],M=n[6],m=n[10],u=n[14],A=n[3],P=n[7],S=n[11],b=n[15],y=r[0],w=r[4],x=r[8],T=r[12],L=r[1],F=r[5],G=r[9],k=r[13],U=r[2],H=r[6],K=r[10],Z=r[14],nt=r[3],X=r[7],j=r[11],et=r[15];return s[0]=a*y+o*L+c*U+l*nt,s[4]=a*w+o*F+c*H+l*X,s[8]=a*x+o*G+c*K+l*j,s[12]=a*T+o*k+c*Z+l*et,s[1]=d*y+p*L+h*U+_*nt,s[5]=d*w+p*F+h*H+_*X,s[9]=d*x+p*G+h*K+_*j,s[13]=d*T+p*k+h*Z+_*et,s[2]=v*y+M*L+m*U+u*nt,s[6]=v*w+M*F+m*H+u*X,s[10]=v*x+M*G+m*K+u*j,s[14]=v*T+M*k+m*Z+u*et,s[3]=A*y+P*L+S*U+b*nt,s[7]=A*w+P*F+S*H+b*X,s[11]=A*x+P*G+S*K+b*j,s[15]=A*T+P*k+S*Z+b*et,this}multiplyScalar(t){const e=this.elements;return e[0]*=t,e[4]*=t,e[8]*=t,e[12]*=t,e[1]*=t,e[5]*=t,e[9]*=t,e[13]*=t,e[2]*=t,e[6]*=t,e[10]*=t,e[14]*=t,e[3]*=t,e[7]*=t,e[11]*=t,e[15]*=t,this}determinant(){const t=this.elements,e=t[0],n=t[4],r=t[8],s=t[12],a=t[1],o=t[5],c=t[9],l=t[13],d=t[2],p=t[6],h=t[10],_=t[14],v=t[3],M=t[7],m=t[11],u=t[15],A=c*_-l*h,P=o*_-l*p,S=o*h-c*p,b=a*_-l*d,y=a*h-c*d,w=a*p-o*d;return e*(M*A-m*P+u*S)-n*(v*A-m*b+u*y)+r*(v*P-M*b+u*w)-s*(v*S-M*y+m*w)}determinantAffine(){const t=this.elements,e=t[0],n=t[4],r=t[8],s=t[1],a=t[5],o=t[9],c=t[2],l=t[6],d=t[10];return e*(a*d-o*l)-n*(s*d-o*c)+r*(s*l-a*c)}transpose(){const t=this.elements;let e;return e=t[1],t[1]=t[4],t[4]=e,e=t[2],t[2]=t[8],t[8]=e,e=t[6],t[6]=t[9],t[9]=e,e=t[3],t[3]=t[12],t[12]=e,e=t[7],t[7]=t[13],t[13]=e,e=t[11],t[11]=t[14],t[14]=e,this}setPosition(t,e,n){const r=this.elements;return t.isVector3?(r[12]=t.x,r[13]=t.y,r[14]=t.z):(r[12]=t,r[13]=e,r[14]=n),this}invert(){const t=this.elements,e=t[0],n=t[1],r=t[2],s=t[3],a=t[4],o=t[5],c=t[6],l=t[7],d=t[8],p=t[9],h=t[10],_=t[11],v=t[12],M=t[13],m=t[14],u=t[15],A=e*o-n*a,P=e*c-r*a,S=e*l-s*a,b=n*c-r*o,y=n*l-s*o,w=r*l-s*c,x=d*M-p*v,T=d*m-h*v,L=d*u-_*v,F=p*m-h*M,G=p*u-_*M,k=h*u-_*m,U=A*k-P*G+S*F+b*L-y*T+w*x;if(U===0)return this.set(0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0);const H=1/U;return t[0]=(o*k-c*G+l*F)*H,t[1]=(r*G-n*k-s*F)*H,t[2]=(M*w-m*y+u*b)*H,t[3]=(h*y-p*w-_*b)*H,t[4]=(c*L-a*k-l*T)*H,t[5]=(e*k-r*L+s*T)*H,t[6]=(m*S-v*w-u*P)*H,t[7]=(d*w-h*S+_*P)*H,t[8]=(a*G-o*L+l*x)*H,t[9]=(n*L-e*G-s*x)*H,t[10]=(v*y-M*S+u*A)*H,t[11]=(p*S-d*y-_*A)*H,t[12]=(o*T-a*F-c*x)*H,t[13]=(e*F-n*T+r*x)*H,t[14]=(M*P-v*b-m*A)*H,t[15]=(d*b-p*P+h*A)*H,this}scale(t){const e=this.elements,n=t.x,r=t.y,s=t.z;return e[0]*=n,e[4]*=r,e[8]*=s,e[1]*=n,e[5]*=r,e[9]*=s,e[2]*=n,e[6]*=r,e[10]*=s,e[3]*=n,e[7]*=r,e[11]*=s,this}getMaxScaleOnAxis(){const t=this.elements,e=t[0]*t[0]+t[1]*t[1]+t[2]*t[2],n=t[4]*t[4]+t[5]*t[5]+t[6]*t[6],r=t[8]*t[8]+t[9]*t[9]+t[10]*t[10];return Math.sqrt(Math.max(e,n,r))}makeTranslation(t,e,n){return t.isVector3?this.set(1,0,0,t.x,0,1,0,t.y,0,0,1,t.z,0,0,0,1):this.set(1,0,0,t,0,1,0,e,0,0,1,n,0,0,0,1),this}makeRotationX(t){const e=Math.cos(t),n=Math.sin(t);return this.set(1,0,0,0,0,e,-n,0,0,n,e,0,0,0,0,1),this}makeRotationY(t){const e=Math.cos(t),n=Math.sin(t);return this.set(e,0,n,0,0,1,0,0,-n,0,e,0,0,0,0,1),this}makeRotationZ(t){const e=Math.cos(t),n=Math.sin(t);return this.set(e,-n,0,0,n,e,0,0,0,0,1,0,0,0,0,1),this}makeRotationAxis(t,e){const n=Math.cos(e),r=Math.sin(e),s=1-n,a=t.x,o=t.y,c=t.z,l=s*a,d=s*o;return this.set(l*a+n,l*o-r*c,l*c+r*o,0,l*o+r*c,d*o+n,d*c-r*a,0,l*c-r*o,d*c+r*a,s*c*c+n,0,0,0,0,1),this}makeScale(t,e,n){return this.set(t,0,0,0,0,e,0,0,0,0,n,0,0,0,0,1),this}makeShear(t,e,n,r,s,a){return this.set(1,n,s,0,t,1,a,0,e,r,1,0,0,0,0,1),this}compose(t,e,n){const r=this.elements,s=e._x,a=e._y,o=e._z,c=e._w,l=s+s,d=a+a,p=o+o,h=s*l,_=s*d,v=s*p,M=a*d,m=a*p,u=o*p,A=c*l,P=c*d,S=c*p,b=n.x,y=n.y,w=n.z;return r[0]=(1-(M+u))*b,r[1]=(_+S)*b,r[2]=(v-P)*b,r[3]=0,r[4]=(_-S)*y,r[5]=(1-(h+u))*y,r[6]=(m+A)*y,r[7]=0,r[8]=(v+P)*w,r[9]=(m-A)*w,r[10]=(1-(h+M))*w,r[11]=0,r[12]=t.x,r[13]=t.y,r[14]=t.z,r[15]=1,this}decompose(t,e,n){const r=this.elements;t.x=r[12],t.y=r[13],t.z=r[14];const s=this.determinantAffine();if(s===0)return n.set(1,1,1),e.identity(),this;let a=$n.set(r[0],r[1],r[2]).length();const o=$n.set(r[4],r[5],r[6]).length(),c=$n.set(r[8],r[9],r[10]).length();s<0&&(a=-a),We.copy(this);const l=1/a,d=1/o,p=1/c;return We.elements[0]*=l,We.elements[1]*=l,We.elements[2]*=l,We.elements[4]*=d,We.elements[5]*=d,We.elements[6]*=d,We.elements[8]*=p,We.elements[9]*=p,We.elements[10]*=p,e.setFromRotationMatrix(We),n.x=a,n.y=o,n.z=c,this}makePerspective(t,e,n,r,s,a,o=an,c=!1){const l=this.elements,d=2*s/(e-t),p=2*s/(n-r),h=(e+t)/(e-t),_=(n+r)/(n-r);let v,M;if(c)v=s/(a-s),M=a*s/(a-s);else if(o===an)v=-(a+s)/(a-s),M=-2*a*s/(a-s);else if(o===zi)v=-a/(a-s),M=-a*s/(a-s);else throw new Error("THREE.Matrix4.makePerspective(): Invalid coordinate system: "+o);return l[0]=d,l[4]=0,l[8]=h,l[12]=0,l[1]=0,l[5]=p,l[9]=_,l[13]=0,l[2]=0,l[6]=0,l[10]=v,l[14]=M,l[3]=0,l[7]=0,l[11]=-1,l[15]=0,this}makeOrthographic(t,e,n,r,s,a,o=an,c=!1){const l=this.elements,d=2/(e-t),p=2/(n-r),h=-(e+t)/(e-t),_=-(n+r)/(n-r);let v,M;if(c)v=1/(a-s),M=a/(a-s);else if(o===an)v=-2/(a-s),M=-(a+s)/(a-s);else if(o===zi)v=-1/(a-s),M=-s/(a-s);else throw new Error("THREE.Matrix4.makeOrthographic(): Invalid coordinate system: "+o);return l[0]=d,l[4]=0,l[8]=0,l[12]=h,l[1]=0,l[5]=p,l[9]=0,l[13]=_,l[2]=0,l[6]=0,l[10]=v,l[14]=M,l[3]=0,l[7]=0,l[11]=0,l[15]=1,this}equals(t){const e=this.elements,n=t.elements;for(let r=0;r<16;r++)if(e[r]!==n[r])return!1;return!0}fromArray(t,e=0){for(let n=0;n<16;n++)this.elements[n]=t[n+e];return this}toArray(t=[],e=0){const n=this.elements;return t[e]=n[0],t[e+1]=n[1],t[e+2]=n[2],t[e+3]=n[3],t[e+4]=n[4],t[e+5]=n[5],t[e+6]=n[6],t[e+7]=n[7],t[e+8]=n[8],t[e+9]=n[9],t[e+10]=n[10],t[e+11]=n[11],t[e+12]=n[12],t[e+13]=n[13],t[e+14]=n[14],t[e+15]=n[15],t}};Fr.prototype.isMatrix4=!0;let ee=Fr;const $n=new I,We=new ee,nh=new I(0,0,0),ih=new I(1,1,1),An=new I,Yi=new I,Fe=new I,eo=new ee,no=new Ln;class In{constructor(t=0,e=0,n=0,r=In.DEFAULT_ORDER){this.isEuler=!0,this._x=t,this._y=e,this._z=n,this._order=r}get x(){return this._x}set x(t){this._x=t,this._onChangeCallback()}get y(){return this._y}set y(t){this._y=t,this._onChangeCallback()}get z(){return this._z}set z(t){this._z=t,this._onChangeCallback()}get order(){return this._order}set order(t){this._order=t,this._onChangeCallback()}set(t,e,n,r=this._order){return this._x=t,this._y=e,this._z=n,this._order=r,this._onChangeCallback(),this}clone(){return new this.constructor(this._x,this._y,this._z,this._order)}copy(t){return this._x=t._x,this._y=t._y,this._z=t._z,this._order=t._order,this._onChangeCallback(),this}setFromRotationMatrix(t,e=this._order,n=!0){const r=t.elements,s=r[0],a=r[4],o=r[8],c=r[1],l=r[5],d=r[9],p=r[2],h=r[6],_=r[10];switch(e){case"XYZ":this._y=Math.asin(Lt(o,-1,1)),Math.abs(o)<.9999999?(this._x=Math.atan2(-d,_),this._z=Math.atan2(-a,s)):(this._x=Math.atan2(h,l),this._z=0);break;case"YXZ":this._x=Math.asin(-Lt(d,-1,1)),Math.abs(d)<.9999999?(this._y=Math.atan2(o,_),this._z=Math.atan2(c,l)):(this._y=Math.atan2(-p,s),this._z=0);break;case"ZXY":this._x=Math.asin(Lt(h,-1,1)),Math.abs(h)<.9999999?(this._y=Math.atan2(-p,_),this._z=Math.atan2(-a,l)):(this._y=0,this._z=Math.atan2(c,s));break;case"ZYX":this._y=Math.asin(-Lt(p,-1,1)),Math.abs(p)<.9999999?(this._x=Math.atan2(h,_),this._z=Math.atan2(c,s)):(this._x=0,this._z=Math.atan2(-a,l));break;case"YZX":this._z=Math.asin(Lt(c,-1,1)),Math.abs(c)<.9999999?(this._x=Math.atan2(-d,l),this._y=Math.atan2(-p,s)):(this._x=0,this._y=Math.atan2(o,_));break;case"XZY":this._z=Math.asin(-Lt(a,-1,1)),Math.abs(a)<.9999999?(this._x=Math.atan2(h,l),this._y=Math.atan2(o,s)):(this._x=Math.atan2(-d,_),this._y=0);break;default:Rt("Euler: .setFromRotationMatrix() encountered an unknown order: "+e)}return this._order=e,n===!0&&this._onChangeCallback(),this}setFromQuaternion(t,e,n){return eo.makeRotationFromQuaternion(t),this.setFromRotationMatrix(eo,e,n)}setFromVector3(t,e=this._order){return this.set(t.x,t.y,t.z,e)}reorder(t){return no.setFromEuler(this),this.setFromQuaternion(no,t)}equals(t){return t._x===this._x&&t._y===this._y&&t._z===this._z&&t._order===this._order}fromArray(t){return this._x=t[0],this._y=t[1],this._z=t[2],t[3]!==void 0&&(this._order=t[3]),this._onChangeCallback(),this}toArray(t=[],e=0){return t[e]=this._x,t[e+1]=this._y,t[e+2]=this._z,t[e+3]=this._order,t}_onChange(t){return this._onChangeCallback=t,this}_onChangeCallback(){}*[Symbol.iterator](){yield this._x,yield this._y,yield this._z,yield this._order}}In.DEFAULT_ORDER="XYZ";class Ta{constructor(){this.mask=1}set(t){this.mask=(1<<t|0)>>>0}enable(t){this.mask|=1<<t|0}enableAll(){this.mask=-1}toggle(t){this.mask^=1<<t|0}disable(t){this.mask&=~(1<<t|0)}disableAll(){this.mask=0}test(t){return(this.mask&t.mask)!==0}isEnabled(t){return(this.mask&(1<<t|0))!==0}}let rh=0;const io=new I,Jn=new Ln,un=new ee,qi=new I,yi=new I,sh=new I,ah=new Ln,ro=new I(1,0,0),so=new I(0,1,0),ao=new I(0,0,1),oo={type:"added"},oh={type:"removed"},Qn={type:"childadded",child:null},Qr={type:"childremoved",child:null};class Se extends Un{constructor(){super(),this.isObject3D=!0,Object.defineProperty(this,"id",{value:rh++}),this.uuid=Sn(),this.name="",this.type="Object3D",this.parent=null,this.children=[],this.up=Se.DEFAULT_UP.clone();const t=new I,e=new In,n=new Ln,r=new I(1,1,1);function s(){n.setFromEuler(e,!1)}function a(){e.setFromQuaternion(n,void 0,!1)}e._onChange(s),n._onChange(a),Object.defineProperties(this,{position:{configurable:!0,enumerable:!0,value:t},rotation:{configurable:!0,enumerable:!0,value:e},quaternion:{configurable:!0,enumerable:!0,value:n},scale:{configurable:!0,enumerable:!0,value:r},modelViewMatrix:{value:new ee},normalMatrix:{value:new Dt}}),this.matrix=new ee,this.matrixWorld=new ee,this.matrixAutoUpdate=Se.DEFAULT_MATRIX_AUTO_UPDATE,this.matrixWorldAutoUpdate=Se.DEFAULT_MATRIX_WORLD_AUTO_UPDATE,this.matrixWorldNeedsUpdate=!1,this.layers=new Ta,this.visible=!0,this.castShadow=!1,this.receiveShadow=!1,this.frustumCulled=!0,this.renderOrder=0,this.animations=[],this.customDepthMaterial=void 0,this.customDistanceMaterial=void 0,this.static=!1,this.userData={},this.pivot=null}onBeforeShadow(){}onAfterShadow(){}onBeforeRender(){}onAfterRender(){}applyMatrix4(t){this.matrixAutoUpdate&&this.updateMatrix(),this.matrix.premultiply(t),this.matrix.decompose(this.position,this.quaternion,this.scale)}applyQuaternion(t){return this.quaternion.premultiply(t),this}setRotationFromAxisAngle(t,e){this.quaternion.setFromAxisAngle(t,e)}setRotationFromEuler(t){this.quaternion.setFromEuler(t,!0)}setRotationFromMatrix(t){this.quaternion.setFromRotationMatrix(t)}setRotationFromQuaternion(t){this.quaternion.copy(t)}rotateOnAxis(t,e){return Jn.setFromAxisAngle(t,e),this.quaternion.multiply(Jn),this}rotateOnWorldAxis(t,e){return Jn.setFromAxisAngle(t,e),this.quaternion.premultiply(Jn),this}rotateX(t){return this.rotateOnAxis(ro,t)}rotateY(t){return this.rotateOnAxis(so,t)}rotateZ(t){return this.rotateOnAxis(ao,t)}translateOnAxis(t,e){return io.copy(t).applyQuaternion(this.quaternion),this.position.add(io.multiplyScalar(e)),this}translateX(t){return this.translateOnAxis(ro,t)}translateY(t){return this.translateOnAxis(so,t)}translateZ(t){return this.translateOnAxis(ao,t)}localToWorld(t){return this.updateWorldMatrix(!0,!1),t.applyMatrix4(this.matrixWorld)}worldToLocal(t){return this.updateWorldMatrix(!0,!1),t.applyMatrix4(un.copy(this.matrixWorld).invert())}lookAt(t,e,n){t.isVector3?qi.copy(t):qi.set(t,e,n);const r=this.parent;this.updateWorldMatrix(!0,!1),yi.setFromMatrixPosition(this.matrixWorld),this.isCamera||this.isLight?un.lookAt(yi,qi,this.up):un.lookAt(qi,yi,this.up),this.quaternion.setFromRotationMatrix(un),r&&(un.extractRotation(r.matrixWorld),Jn.setFromRotationMatrix(un),this.quaternion.premultiply(Jn.invert()))}add(t){if(arguments.length>1){for(let e=0;e<arguments.length;e++)this.add(arguments[e]);return this}return t===this?(Wt("Object3D.add: object can't be added as a child of itself.",t),this):(t&&t.isObject3D?(t.removeFromParent(),t.parent=this,this.children.push(t),t.dispatchEvent(oo),Qn.child=t,this.dispatchEvent(Qn),Qn.child=null):Wt("Object3D.add: object not an instance of THREE.Object3D.",t),this)}remove(t){if(arguments.length>1){for(let n=0;n<arguments.length;n++)this.remove(arguments[n]);return this}const e=this.children.indexOf(t);return e!==-1&&(t.parent=null,this.children.splice(e,1),t.dispatchEvent(oh),Qr.child=t,this.dispatchEvent(Qr),Qr.child=null),this}removeFromParent(){const t=this.parent;return t!==null&&t.remove(this),this}clear(){return this.remove(...this.children)}attach(t){return this.updateWorldMatrix(!0,!1),un.copy(this.matrixWorld).invert(),t.parent!==null&&(t.parent.updateWorldMatrix(!0,!1),un.multiply(t.parent.matrixWorld)),t.applyMatrix4(un),t.removeFromParent(),t.parent=this,this.children.push(t),t.updateWorldMatrix(!1,!0),t.dispatchEvent(oo),Qn.child=t,this.dispatchEvent(Qn),Qn.child=null,this}getObjectById(t){return this.getObjectByProperty("id",t)}getObjectByName(t){return this.getObjectByProperty("name",t)}getObjectByProperty(t,e){if(this[t]===e)return this;for(let n=0,r=this.children.length;n<r;n++){const a=this.children[n].getObjectByProperty(t,e);if(a!==void 0)return a}}getObjectsByProperty(t,e,n=[]){this[t]===e&&n.push(this);const r=this.children;for(let s=0,a=r.length;s<a;s++)r[s].getObjectsByProperty(t,e,n);return n}getWorldPosition(t){return this.updateWorldMatrix(!0,!1),t.setFromMatrixPosition(this.matrixWorld)}getWorldQuaternion(t){return this.updateWorldMatrix(!0,!1),this.matrixWorld.decompose(yi,t,sh),t}getWorldScale(t){return this.updateWorldMatrix(!0,!1),this.matrixWorld.decompose(yi,ah,t),t}getWorldDirection(t){this.updateWorldMatrix(!0,!1);const e=this.matrixWorld.elements;return t.set(e[8],e[9],e[10]).normalize()}raycast(){}intersectsFrustum(){}traverse(t){t(this);const e=this.children;for(let n=0,r=e.length;n<r;n++)e[n].traverse(t)}traverseVisible(t){if(this.visible===!1)return;t(this);const e=this.children;for(let n=0,r=e.length;n<r;n++)e[n].traverseVisible(t)}traverseAncestors(t){const e=this.parent;e!==null&&(t(e),e.traverseAncestors(t))}updateMatrix(){this.matrix.compose(this.position,this.quaternion,this.scale);const t=this.pivot;if(t!==null){const e=t.x,n=t.y,r=t.z,s=this.matrix.elements;s[12]+=e-s[0]*e-s[4]*n-s[8]*r,s[13]+=n-s[1]*e-s[5]*n-s[9]*r,s[14]+=r-s[2]*e-s[6]*n-s[10]*r}this.matrixWorldNeedsUpdate=!0}updateMatrixWorld(t){this.matrixAutoUpdate&&this.updateMatrix(),(this.matrixWorldNeedsUpdate||t)&&(this.matrixWorldAutoUpdate===!0&&(this.parent===null?this.matrixWorld.copy(this.matrix):this.matrixWorld.multiplyMatrices(this.parent.matrixWorld,this.matrix)),this.matrixWorldNeedsUpdate=!1,t=!0);const e=this.children;for(let n=0,r=e.length;n<r;n++)e[n].updateMatrixWorld(t)}updateWorldMatrix(t,e,n=!1){const r=this.parent;if(t===!0&&r!==null&&r.updateWorldMatrix(!0,!1),this.matrixAutoUpdate&&this.updateMatrix(),(this.matrixWorldNeedsUpdate||n)&&(this.matrixWorldAutoUpdate===!0&&(this.parent===null?this.matrixWorld.copy(this.matrix):this.matrixWorld.multiplyMatrices(this.parent.matrixWorld,this.matrix)),this.matrixWorldNeedsUpdate=!1,n=!0),e===!0){const s=this.children;for(let a=0,o=s.length;a<o;a++)s[a].updateWorldMatrix(!1,!0,n)}}toJSON(t){const e=t===void 0||typeof t=="string",n={};e&&(t={geometries:{},materials:{},textures:{},images:{},shapes:{},skeletons:{},animations:{},nodes:{}},n.metadata={version:4.7,type:"Object",generator:"Object3D.toJSON"});const r={};r.uuid=this.uuid,r.type=this.type,r.name=this.name,r.castShadow=this.castShadow,r.receiveShadow=this.receiveShadow,r.visible=this.visible,r.frustumCulled=this.frustumCulled,r.renderOrder=this.renderOrder,r.static=this.static,r.matrixAutoUpdate=this.matrixAutoUpdate,Object.keys(this.userData).length>0&&(r.userData=this.userData),r.layers=this.layers.mask,r.matrix=this.matrix.toArray(),r.up=this.up.toArray(),this.pivot!==null&&(r.pivot=this.pivot.toArray()),this.morphTargetDictionary!==void 0&&(r.morphTargetDictionary=Object.assign({},this.morphTargetDictionary)),this.morphTargetInfluences!==void 0&&(r.morphTargetInfluences=this.morphTargetInfluences.slice()),this.isInstancedMesh&&(r.type="InstancedMesh",r.count=this.count,r.instanceMatrix=this.instanceMatrix.toJSON(),this.instanceColor!==null&&(r.instanceColor=this.instanceColor.toJSON())),this.isBatchedMesh&&(r.type="BatchedMesh",r.perObjectFrustumCulled=this.perObjectFrustumCulled,r.sortObjects=this.sortObjects,r.drawRanges=this._drawRanges,r.reservedRanges=this._reservedRanges,r.geometryInfo=this._geometryInfo.map(o=>({...o,boundingBox:o.boundingBox?o.boundingBox.toJSON():void 0,boundingSphere:o.boundingSphere?o.boundingSphere.toJSON():void 0})),r.instanceInfo=this._instanceInfo.map(o=>({...o})),r.availableInstanceIds=this._availableInstanceIds.slice(),r.availableGeometryIds=this._availableGeometryIds.slice(),r.nextIndexStart=this._nextIndexStart,r.nextVertexStart=this._nextVertexStart,r.geometryCount=this._geometryCount,r.maxInstanceCount=this._maxInstanceCount,r.maxVertexCount=this._maxVertexCount,r.maxIndexCount=this._maxIndexCount,r.geometryInitialized=this._geometryInitialized,r.matricesTexture=this._matricesTexture.toJSON(t),r.indirectTexture=this._indirectTexture.toJSON(t),this._colorsTexture!==null&&(r.colorsTexture=this._colorsTexture.toJSON(t)),this.boundingSphere!==null&&(r.boundingSphere=this.boundingSphere.toJSON()),this.boundingBox!==null&&(r.boundingBox=this.boundingBox.toJSON()));function s(o,c){return o[c.uuid]===void 0&&(o[c.uuid]=c.toJSON(t)),c.uuid}if(this.isScene)this.background&&(this.background.isColor?r.background=this.background.toJSON():this.background.isTexture&&(r.background=this.background.toJSON(t).uuid)),this.environment&&this.environment.isTexture&&this.environment.isRenderTargetTexture!==!0&&(r.environment=this.environment.toJSON(t).uuid);else if(this.isMesh||this.isLine||this.isPoints){r.geometry=s(t.geometries,this.geometry);const o=this.geometry.parameters;if(o!==void 0&&o.shapes!==void 0){const c=o.shapes;if(Array.isArray(c))for(let l=0,d=c.length;l<d;l++){const p=c[l];s(t.shapes,p)}else s(t.shapes,c)}}if(this.isSkinnedMesh&&(r.bindMode=this.bindMode,r.bindMatrix=this.bindMatrix.toArray(),this.skeleton!==void 0&&(s(t.skeletons,this.skeleton),r.skeleton=this.skeleton.uuid)),this.material!==void 0)if(Array.isArray(this.material)){const o=[];for(let c=0,l=this.material.length;c<l;c++)o.push(s(t.materials,this.material[c]));r.material=o}else r.material=s(t.materials,this.material);if(this.children.length>0){r.children=[];for(let o=0;o<this.children.length;o++)r.children.push(this.children[o].toJSON(t).object)}if(this.animations.length>0){r.animations=[];for(let o=0;o<this.animations.length;o++){const c=this.animations[o];r.animations.push(s(t.animations,c))}}if(e){const o=a(t.geometries),c=a(t.materials),l=a(t.textures),d=a(t.images),p=a(t.shapes),h=a(t.skeletons),_=a(t.animations),v=a(t.nodes);o.length>0&&(n.geometries=o),c.length>0&&(n.materials=c),l.length>0&&(n.textures=l),d.length>0&&(n.images=d),p.length>0&&(n.shapes=p),h.length>0&&(n.skeletons=h),_.length>0&&(n.animations=_),v.length>0&&(n.nodes=v)}return n.object=r,n;function a(o){const c=[];for(const l in o){const d=o[l];delete d.metadata,c.push(d)}return c}}clone(t){return new this.constructor().copy(this,t)}copy(t,e=!0){if(this.name=t.name,this.up.copy(t.up),this.position.copy(t.position),this.rotation.order=t.rotation.order,this.quaternion.copy(t.quaternion),this.scale.copy(t.scale),this.pivot=t.pivot!==null?t.pivot.clone():null,this.matrix.copy(t.matrix),this.matrixWorld.copy(t.matrixWorld),this.matrixAutoUpdate=t.matrixAutoUpdate,this.matrixWorldAutoUpdate=t.matrixWorldAutoUpdate,this.matrixWorldNeedsUpdate=t.matrixWorldNeedsUpdate,this.layers.mask=t.layers.mask,this.visible=t.visible,this.castShadow=t.castShadow,this.receiveShadow=t.receiveShadow,this.frustumCulled=t.frustumCulled,this.renderOrder=t.renderOrder,this.static=t.static,this.animations=t.animations.slice(),this.userData=JSON.parse(JSON.stringify(t.userData)),e===!0)for(let n=0;n<t.children.length;n++){const r=t.children[n];this.add(r.clone())}return this}dispose(){this.dispatchEvent({type:"dispose"})}}Se.DEFAULT_UP=new I(0,1,0);Se.DEFAULT_MATRIX_AUTO_UPDATE=!0;Se.DEFAULT_MATRIX_WORLD_AUTO_UPDATE=!0;class Zi extends Se{constructor(){super(),this.isGroup=!0,this.type="Group"}}const lh={type:"move"};class jr{constructor(){this._targetRay=null,this._grip=null,this._hand=null}getHandSpace(){return this._hand===null&&(this._hand=new Zi,this._hand.matrixAutoUpdate=!1,this._hand.visible=!1,this._hand.joints={},this._hand.inputState={pinching:!1}),this._hand}getTargetRaySpace(){return this._targetRay===null&&(this._targetRay=new Zi,this._targetRay.matrixAutoUpdate=!1,this._targetRay.visible=!1,this._targetRay.hasLinearVelocity=!1,this._targetRay.linearVelocity=new I,this._targetRay.hasAngularVelocity=!1,this._targetRay.angularVelocity=new I),this._targetRay}getGripSpace(){return this._grip===null&&(this._grip=new Zi,this._grip.matrixAutoUpdate=!1,this._grip.visible=!1,this._grip.hasLinearVelocity=!1,this._grip.linearVelocity=new I,this._grip.hasAngularVelocity=!1,this._grip.angularVelocity=new I,this._grip.eventsEnabled=!1),this._grip}dispatchEvent(t){return this._targetRay!==null&&this._targetRay.dispatchEvent(t),this._grip!==null&&this._grip.dispatchEvent(t),this._hand!==null&&this._hand.dispatchEvent(t),this}connect(t){if(t&&t.hand){const e=this._hand;if(e)for(const n of t.hand.values())this._getHandJoint(e,n)}return this.dispatchEvent({type:"connected",data:t}),this}disconnect(t){return this.dispatchEvent({type:"disconnected",data:t}),this._targetRay!==null&&(this._targetRay.visible=!1),this._grip!==null&&(this._grip.visible=!1),this._hand!==null&&(this._hand.visible=!1),this}update(t,e,n){let r=null,s=null,a=null;const o=this._targetRay,c=this._grip,l=this._hand;if(t&&e.session.visibilityState!=="visible-blurred"){if(l&&t.hand){a=!0;for(const M of t.hand.values()){const m=e.getJointPose(M,n),u=this._getHandJoint(l,M);m!==null&&(u.matrix.fromArray(m.transform.matrix),u.matrix.decompose(u.position,u.rotation,u.scale),u.matrixWorldNeedsUpdate=!0,u.jointRadius=m.radius),u.visible=m!==null}const d=l.joints["index-finger-tip"],p=l.joints["thumb-tip"],h=d.position.distanceTo(p.position),_=.02,v=.005;l.inputState.pinching&&h>_+v?(l.inputState.pinching=!1,this.dispatchEvent({type:"pinchend",handedness:t.handedness,target:this})):!l.inputState.pinching&&h<=_-v&&(l.inputState.pinching=!0,this.dispatchEvent({type:"pinchstart",handedness:t.handedness,target:this}))}else c!==null&&t.gripSpace&&(s=e.getPose(t.gripSpace,n),s!==null&&(c.matrix.fromArray(s.transform.matrix),c.matrix.decompose(c.position,c.rotation,c.scale),c.matrixWorldNeedsUpdate=!0,s.linearVelocity?(c.hasLinearVelocity=!0,c.linearVelocity.copy(s.linearVelocity)):c.hasLinearVelocity=!1,s.angularVelocity?(c.hasAngularVelocity=!0,c.angularVelocity.copy(s.angularVelocity)):c.hasAngularVelocity=!1,c.eventsEnabled&&c.dispatchEvent({type:"gripUpdated",data:t,target:this})));o!==null&&(r=e.getPose(t.targetRaySpace,n),r===null&&s!==null&&(r=s),r!==null&&(o.matrix.fromArray(r.transform.matrix),o.matrix.decompose(o.position,o.rotation,o.scale),o.matrixWorldNeedsUpdate=!0,r.linearVelocity?(o.hasLinearVelocity=!0,o.linearVelocity.copy(r.linearVelocity)):o.hasLinearVelocity=!1,r.angularVelocity?(o.hasAngularVelocity=!0,o.angularVelocity.copy(r.angularVelocity)):o.hasAngularVelocity=!1,this.dispatchEvent(lh)))}return o!==null&&(o.visible=r!==null),c!==null&&(c.visible=s!==null),l!==null&&(l.visible=a!==null),this}_getHandJoint(t,e){if(t.joints[e.jointName]===void 0){const n=new Zi;n.matrixAutoUpdate=!1,n.visible=!1,t.joints[e.jointName]=n,t.add(n)}return t.joints[e.jointName]}}const El={aliceblue:15792383,antiquewhite:16444375,aqua:65535,aquamarine:8388564,azure:15794175,beige:16119260,bisque:16770244,black:0,blanchedalmond:16772045,blue:255,blueviolet:9055202,brown:10824234,burlywood:14596231,cadetblue:6266528,chartreuse:8388352,chocolate:13789470,coral:16744272,cornflowerblue:6591981,cornsilk:16775388,crimson:14423100,cyan:65535,darkblue:139,darkcyan:35723,darkgoldenrod:12092939,darkgray:11119017,darkgreen:25600,darkgrey:11119017,darkkhaki:12433259,darkmagenta:9109643,darkolivegreen:5597999,darkorange:16747520,darkorchid:10040012,darkred:9109504,darksalmon:15308410,darkseagreen:9419919,darkslateblue:4734347,darkslategray:3100495,darkslategrey:3100495,darkturquoise:52945,darkviolet:9699539,deeppink:16716947,deepskyblue:49151,dimgray:6908265,dimgrey:6908265,dodgerblue:2003199,firebrick:11674146,floralwhite:16775920,forestgreen:2263842,fuchsia:16711935,gainsboro:14474460,ghostwhite:16316671,gold:16766720,goldenrod:14329120,gray:8421504,green:32768,greenyellow:11403055,grey:8421504,honeydew:15794160,hotpink:16738740,indianred:13458524,indigo:4915330,ivory:16777200,khaki:15787660,lavender:15132410,lavenderblush:16773365,lawngreen:8190976,lemonchiffon:16775885,lightblue:11393254,lightcoral:15761536,lightcyan:14745599,lightgoldenrodyellow:16448210,lightgray:13882323,lightgreen:9498256,lightgrey:13882323,lightpink:16758465,lightsalmon:16752762,lightseagreen:2142890,lightskyblue:8900346,lightslategray:7833753,lightslategrey:7833753,lightsteelblue:11584734,lightyellow:16777184,lime:65280,limegreen:3329330,linen:16445670,magenta:16711935,maroon:8388608,mediumaquamarine:6737322,mediumblue:205,mediumorchid:12211667,mediumpurple:9662683,mediumseagreen:3978097,mediumslateblue:8087790,mediumspringgreen:64154,mediumturquoise:4772300,mediumvioletred:13047173,midnightblue:1644912,mintcream:16121850,mistyrose:16770273,moccasin:16770229,navajowhite:16768685,navy:128,oldlace:16643558,olive:8421376,olivedrab:7048739,orange:16753920,orangered:16729344,orchid:14315734,palegoldenrod:15657130,palegreen:10025880,paleturquoise:11529966,palevioletred:14381203,papayawhip:16773077,peachpuff:16767673,peru:13468991,pink:16761035,plum:14524637,powderblue:11591910,purple:8388736,rebeccapurple:6697881,red:16711680,rosybrown:12357519,royalblue:4286945,saddlebrown:9127187,salmon:16416882,sandybrown:16032864,seagreen:3050327,seashell:16774638,sienna:10506797,silver:12632256,skyblue:8900331,slateblue:6970061,slategray:7372944,slategrey:7372944,snow:16775930,springgreen:65407,steelblue:4620980,tan:13808780,teal:32896,thistle:14204888,tomato:16737095,turquoise:4251856,violet:15631086,wheat:16113331,white:16777215,whitesmoke:16119285,yellow:16776960,yellowgreen:10145074},wn={h:0,s:0,l:0},Ki={h:0,s:0,l:0};function ts(i,t,e){return e<0&&(e+=1),e>1&&(e-=1),e<1/6?i+(t-i)*6*e:e<1/2?t:e<2/3?i+(t-i)*6*(2/3-e):i}class Gt{constructor(t,e,n){return this.isColor=!0,this.r=1,this.g=1,this.b=1,this.set(t,e,n)}set(t,e,n){if(e===void 0&&n===void 0){const r=t;r&&r.isColor?this.copy(r):typeof r=="number"?this.setHex(r):typeof r=="string"&&this.setStyle(r)}else this.setRGB(t,e,n);return this}setScalar(t){return this.r=t,this.g=t,this.b=t,this}setHex(t,e=He){return t=Math.floor(t),this.r=(t>>16&255)/255,this.g=(t>>8&255)/255,this.b=(t&255)/255,Ht.colorSpaceToWorking(this,e),this}setRGB(t,e,n,r=Ht.workingColorSpace){return this.r=t,this.g=e,this.b=n,Ht.colorSpaceToWorking(this,r),this}setHSL(t,e,n,r=Ht.workingColorSpace){if(t=ya(t,1),e=Lt(e,0,1),n=Lt(n,0,1),e===0)this.r=this.g=this.b=n;else{const s=n<=.5?n*(1+e):n+e-n*e,a=2*n-s;this.r=ts(a,s,t+1/3),this.g=ts(a,s,t),this.b=ts(a,s,t-1/3)}return Ht.colorSpaceToWorking(this,r),this}setStyle(t,e=He){function n(s){s!==void 0&&parseFloat(s)<1&&Rt("Color: Alpha component of "+t+" will be ignored.")}let r;if(r=/^(\w+)\(([^\)]*)\)/.exec(t)){let s;const a=r[1],o=r[2];switch(a){case"rgb":case"rgba":if(s=/^\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*(?:,\s*(\d*\.?\d+)\s*)?$/.exec(o))return n(s[4]),this.setRGB(Math.min(255,parseInt(s[1],10))/255,Math.min(255,parseInt(s[2],10))/255,Math.min(255,parseInt(s[3],10))/255,e);if(s=/^\s*(\d+)\%\s*,\s*(\d+)\%\s*,\s*(\d+)\%\s*(?:,\s*(\d*\.?\d+)\s*)?$/.exec(o))return n(s[4]),this.setRGB(Math.min(100,parseInt(s[1],10))/100,Math.min(100,parseInt(s[2],10))/100,Math.min(100,parseInt(s[3],10))/100,e);break;case"hsl":case"hsla":if(s=/^\s*(\d*\.?\d+)\s*,\s*(\d*\.?\d+)\%\s*,\s*(\d*\.?\d+)\%\s*(?:,\s*(\d*\.?\d+)\s*)?$/.exec(o))return n(s[4]),this.setHSL(parseFloat(s[1])/360,parseFloat(s[2])/100,parseFloat(s[3])/100,e);break;default:Rt("Color: Unknown color model "+t)}}else if(r=/^\#([A-Fa-f\d]+)$/.exec(t)){const s=r[1],a=s.length;if(a===3)return this.setRGB(parseInt(s.charAt(0),16)/15,parseInt(s.charAt(1),16)/15,parseInt(s.charAt(2),16)/15,e);if(a===6)return this.setHex(parseInt(s,16),e);Rt("Color: Invalid hex color "+t)}else if(t&&t.length>0)return this.setColorName(t,e);return this}setColorName(t,e=He){const n=El[t.toLowerCase()];return n!==void 0?this.setHex(n,e):Rt("Color: Unknown color "+t),this}clone(){return new this.constructor(this.r,this.g,this.b)}copy(t){return this.r=t.r,this.g=t.g,this.b=t.b,this}copySRGBToLinear(t){return this.r=Mn(t.r),this.g=Mn(t.g),this.b=Mn(t.b),this}copyLinearToSRGB(t){return this.r=gi(t.r),this.g=gi(t.g),this.b=gi(t.b),this}convertSRGBToLinear(){return this.copySRGBToLinear(this),this}convertLinearToSRGB(){return this.copyLinearToSRGB(this),this}getHex(t=He){return Ht.workingToColorSpace(Te.copy(this),t),Math.round(Lt(Te.r*255,0,255))*65536+Math.round(Lt(Te.g*255,0,255))*256+Math.round(Lt(Te.b*255,0,255))}getHexString(t=He){return("000000"+this.getHex(t).toString(16)).slice(-6)}getHSL(t,e=Ht.workingColorSpace){Ht.workingToColorSpace(Te.copy(this),e);const n=Te.r,r=Te.g,s=Te.b,a=Math.max(n,r,s),o=Math.min(n,r,s);let c,l;const d=(o+a)/2;if(o===a)c=0,l=0;else{const p=a-o;switch(l=d<=.5?p/(a+o):p/(2-a-o),a){case n:c=(r-s)/p+(r<s?6:0);break;case r:c=(s-n)/p+2;break;case s:c=(n-r)/p+4;break}c/=6}return t.h=c,t.s=l,t.l=d,t}getRGB(t,e=Ht.workingColorSpace){return Ht.workingToColorSpace(Te.copy(this),e),t.r=Te.r,t.g=Te.g,t.b=Te.b,t}getStyle(t=He){Ht.workingToColorSpace(Te.copy(this),t);const e=Te.r,n=Te.g,r=Te.b;return t!==He?`color(${t} ${e.toFixed(3)} ${n.toFixed(3)} ${r.toFixed(3)})`:`rgb(${Math.round(e*255)},${Math.round(n*255)},${Math.round(r*255)})`}offsetHSL(t,e,n){return this.getHSL(wn),this.setHSL(wn.h+t,wn.s+e,wn.l+n)}add(t){return this.r+=t.r,this.g+=t.g,this.b+=t.b,this}addColors(t,e){return this.r=t.r+e.r,this.g=t.g+e.g,this.b=t.b+e.b,this}addScalar(t){return this.r+=t,this.g+=t,this.b+=t,this}sub(t){return this.r=Math.max(0,this.r-t.r),this.g=Math.max(0,this.g-t.g),this.b=Math.max(0,this.b-t.b),this}multiply(t){return this.r*=t.r,this.g*=t.g,this.b*=t.b,this}multiplyScalar(t){return this.r*=t,this.g*=t,this.b*=t,this}lerp(t,e){return this.r+=(t.r-this.r)*e,this.g+=(t.g-this.g)*e,this.b+=(t.b-this.b)*e,this}lerpColors(t,e,n){return this.r=t.r+(e.r-t.r)*n,this.g=t.g+(e.g-t.g)*n,this.b=t.b+(e.b-t.b)*n,this}lerpHSL(t,e){this.getHSL(wn),t.getHSL(Ki);const n=Ni(wn.h,Ki.h,e),r=Ni(wn.s,Ki.s,e),s=Ni(wn.l,Ki.l,e);return this.setHSL(n,r,s),this}setFromVector3(t){return this.r=t.x,this.g=t.y,this.b=t.z,this}applyMatrix3(t){const e=this.r,n=this.g,r=this.b,s=t.elements;return this.r=s[0]*e+s[3]*n+s[6]*r,this.g=s[1]*e+s[4]*n+s[7]*r,this.b=s[2]*e+s[5]*n+s[8]*r,this}equals(t){return t.r===this.r&&t.g===this.g&&t.b===this.b}fromArray(t,e=0){return this.r=t[e],this.g=t[e+1],this.b=t[e+2],this}toArray(t=[],e=0){return t[e]=this.r,t[e+1]=this.g,t[e+2]=this.b,t}fromBufferAttribute(t,e){return this.r=t.getX(e),this.g=t.getY(e),this.b=t.getZ(e),this}toJSON(){return this.getHex()}*[Symbol.iterator](){yield this.r,yield this.g,yield this.b}}const Te=new Gt;Gt.NAMES=El;class yl{constructor(t,e=1,n=1e3){this.isFog=!0,this.name="",this.color=new Gt(t),this.near=e,this.far=n}clone(){return new yl(this.color,this.near,this.far)}toJSON(){return{type:"Fog",name:this.name,color:this.color.getHex(),near:this.near,far:this.far}}}class Zm extends Se{constructor(){super(),this.isScene=!0,this.type="Scene",this.background=null,this.environment=null,this.fog=null,this.backgroundBlurriness=0,this.backgroundIntensity=1,this.backgroundRotation=new In,this.environmentIntensity=1,this.environmentRotation=new In,this.overrideMaterial=null,typeof __THREE_DEVTOOLS__<"u"&&__THREE_DEVTOOLS__.dispatchEvent(new CustomEvent("observe",{detail:this}))}copy(t,e){return super.copy(t,e),t.background!==null&&(this.background=t.background.clone()),t.environment!==null&&(this.environment=t.environment.clone()),t.fog!==null&&(this.fog=t.fog.clone()),this.backgroundBlurriness=t.backgroundBlurriness,this.backgroundIntensity=t.backgroundIntensity,this.backgroundRotation.copy(t.backgroundRotation),this.environmentIntensity=t.environmentIntensity,this.environmentRotation.copy(t.environmentRotation),t.overrideMaterial!==null&&(this.overrideMaterial=t.overrideMaterial.clone()),this.matrixAutoUpdate=t.matrixAutoUpdate,this}toJSON(t){const e=super.toJSON(t);return this.fog!==null&&(e.object.fog=this.fog.toJSON()),e.object.backgroundBlurriness=this.backgroundBlurriness,e.object.backgroundIntensity=this.backgroundIntensity,e.object.backgroundRotation=this.backgroundRotation.toArray(),e.object.environmentIntensity=this.environmentIntensity,e.object.environmentRotation=this.environmentRotation.toArray(),e}}const Xe=new I,dn=new I,es=new I,fn=new I,jn=new I,ti=new I,lo=new I,ns=new I,is=new I,rs=new I,ss=new ce,as=new ce,os=new ce;class Ve{constructor(t=new I,e=new I,n=new I){this.a=t,this.b=e,this.c=n}static getNormal(t,e,n,r){r.subVectors(n,e),Xe.subVectors(t,e),r.cross(Xe);const s=r.lengthSq();return s>0?r.multiplyScalar(1/Math.sqrt(s)):r.set(0,0,0)}static getBarycoord(t,e,n,r,s){Xe.subVectors(r,e),dn.subVectors(n,e),es.subVectors(t,e);const a=Xe.dot(Xe),o=Xe.dot(dn),c=Xe.dot(es),l=dn.dot(dn),d=dn.dot(es),p=a*l-o*o;if(p===0)return s.set(0,0,0),null;const h=1/p,_=(l*c-o*d)*h,v=(a*d-o*c)*h;return s.set(1-_-v,v,_)}static containsPoint(t,e,n,r){return this.getBarycoord(t,e,n,r,fn)===null?!1:fn.x>=0&&fn.y>=0&&fn.x+fn.y<=1}static getInterpolation(t,e,n,r,s,a,o,c){return this.getBarycoord(t,e,n,r,fn)===null?(c.x=0,c.y=0,"z"in c&&(c.z=0),"w"in c&&(c.w=0),null):(c.setScalar(0),c.addScaledVector(s,fn.x),c.addScaledVector(a,fn.y),c.addScaledVector(o,fn.z),c)}static getInterpolatedAttribute(t,e,n,r,s,a){return ss.setScalar(0),as.setScalar(0),os.setScalar(0),ss.fromBufferAttribute(t,e),as.fromBufferAttribute(t,n),os.fromBufferAttribute(t,r),a.setScalar(0),a.addScaledVector(ss,s.x),a.addScaledVector(as,s.y),a.addScaledVector(os,s.z),a}static isFrontFacing(t,e,n,r){return Xe.subVectors(n,e),dn.subVectors(t,e),Xe.cross(dn).dot(r)<0}set(t,e,n){return this.a.copy(t),this.b.copy(e),this.c.copy(n),this}setFromPointsAndIndices(t,e,n,r){return this.a.copy(t[e]),this.b.copy(t[n]),this.c.copy(t[r]),this}setFromAttributeAndIndices(t,e,n,r){return this.a.fromBufferAttribute(t,e),this.b.fromBufferAttribute(t,n),this.c.fromBufferAttribute(t,r),this}clone(){return new this.constructor().copy(this)}copy(t){return this.a.copy(t.a),this.b.copy(t.b),this.c.copy(t.c),this}getArea(){return Xe.subVectors(this.c,this.b),dn.subVectors(this.a,this.b),Xe.cross(dn).length()*.5}getMidpoint(t){return t.addVectors(this.a,this.b).add(this.c).multiplyScalar(1/3)}getNormal(t){return Ve.getNormal(this.a,this.b,this.c,t)}getPlane(t){return t.setFromCoplanarPoints(this.a,this.b,this.c)}getBarycoord(t,e){return Ve.getBarycoord(t,this.a,this.b,this.c,e)}getInterpolation(t,e,n,r,s){return Ve.getInterpolation(t,this.a,this.b,this.c,e,n,r,s)}containsPoint(t){return Ve.containsPoint(t,this.a,this.b,this.c)}isFrontFacing(t){return Ve.isFrontFacing(this.a,this.b,this.c,t)}intersectsBox(t){return t.intersectsTriangle(this)}closestPointToPoint(t,e){const n=this.a,r=this.b,s=this.c;let a,o;jn.subVectors(r,n),ti.subVectors(s,n),ns.subVectors(t,n);const c=jn.dot(ns),l=ti.dot(ns);if(c<=0&&l<=0)return e.copy(n);is.subVectors(t,r);const d=jn.dot(is),p=ti.dot(is);if(d>=0&&p<=d)return e.copy(r);const h=c*p-d*l;if(h<=0&&c>=0&&d<=0)return a=c/(c-d),e.copy(n).addScaledVector(jn,a);rs.subVectors(t,s);const _=jn.dot(rs),v=ti.dot(rs);if(v>=0&&_<=v)return e.copy(s);const M=_*l-c*v;if(M<=0&&l>=0&&v<=0)return o=l/(l-v),e.copy(n).addScaledVector(ti,o);const m=d*v-_*p;if(m<=0&&p-d>=0&&_-v>=0)return lo.subVectors(s,r),o=(p-d)/(p-d+(_-v)),e.copy(r).addScaledVector(lo,o);const u=1/(m+M+h);return a=M*u,o=h*u,e.copy(n).addScaledVector(jn,a).addScaledVector(ti,o)}equals(t){return t.a.equals(this.a)&&t.b.equals(this.b)&&t.c.equals(this.c)}}class Yn{constructor(t=new I(1/0,1/0,1/0),e=new I(-1/0,-1/0,-1/0)){this.isBox3=!0,this.min=t,this.max=e}set(t,e){return this.min.copy(t),this.max.copy(e),this}setFromArray(t){this.makeEmpty();for(let e=0,n=t.length;e<n;e+=3)this.expandByPoint(Ye.fromArray(t,e));return this}setFromBufferAttribute(t){this.makeEmpty();for(let e=0,n=t.count;e<n;e++)this.expandByPoint(Ye.fromBufferAttribute(t,e));return this}setFromPoints(t){this.makeEmpty();for(let e=0,n=t.length;e<n;e++)this.expandByPoint(t[e]);return this}setFromCenterAndSize(t,e){const n=Ye.copy(e).multiplyScalar(.5);return this.min.copy(t).sub(n),this.max.copy(t).add(n),this}setFromObject(t,e=!1){return this.makeEmpty(),this.expandByObject(t,e)}clone(){return new this.constructor().copy(this)}copy(t){return this.min.copy(t.min),this.max.copy(t.max),this}makeEmpty(){return this.min.x=this.min.y=this.min.z=1/0,this.max.x=this.max.y=this.max.z=-1/0,this}isEmpty(){return this.max.x<this.min.x||this.max.y<this.min.y||this.max.z<this.min.z}getCenter(t){return this.isEmpty()?t.set(0,0,0):t.addVectors(this.min,this.max).multiplyScalar(.5)}getSize(t){return this.isEmpty()?t.set(0,0,0):t.subVectors(this.max,this.min)}expandByPoint(t){return this.min.min(t),this.max.max(t),this}expandByVector(t){return this.min.sub(t),this.max.add(t),this}expandByScalar(t){return this.min.addScalar(-t),this.max.addScalar(t),this}expandByObject(t,e=!1){t.updateWorldMatrix(!1,!1);const n=t.geometry;if(n!==void 0){const s=n.getAttribute("position");if(e===!0&&s!==void 0&&t.isInstancedMesh!==!0)for(let a=0,o=s.count;a<o;a++)t.isMesh===!0?t.getVertexPosition(a,Ye):Ye.fromBufferAttribute(s,a),Ye.applyMatrix4(t.matrixWorld),this.expandByPoint(Ye);else t.boundingBox!==void 0?(t.boundingBox===null&&t.computeBoundingBox(),$i.copy(t.boundingBox)):(n.boundingBox===null&&n.computeBoundingBox(),$i.copy(n.boundingBox)),$i.applyMatrix4(t.matrixWorld),this.union($i)}const r=t.children;for(let s=0,a=r.length;s<a;s++)this.expandByObject(r[s],e);return this}containsPoint(t){return t.x>=this.min.x&&t.x<=this.max.x&&t.y>=this.min.y&&t.y<=this.max.y&&t.z>=this.min.z&&t.z<=this.max.z}containsBox(t){return this.min.x<=t.min.x&&t.max.x<=this.max.x&&this.min.y<=t.min.y&&t.max.y<=this.max.y&&this.min.z<=t.min.z&&t.max.z<=this.max.z}getParameter(t,e){return e.set((t.x-this.min.x)/(this.max.x-this.min.x),(t.y-this.min.y)/(this.max.y-this.min.y),(t.z-this.min.z)/(this.max.z-this.min.z))}intersectsBox(t){return t.max.x>=this.min.x&&t.min.x<=this.max.x&&t.max.y>=this.min.y&&t.min.y<=this.max.y&&t.max.z>=this.min.z&&t.min.z<=this.max.z}intersectsSphere(t){return this.clampPoint(t.center,Ye),Ye.distanceToSquared(t.center)<=t.radius*t.radius}intersectsPlane(t){let e,n;return t.normal.x>0?(e=t.normal.x*this.min.x,n=t.normal.x*this.max.x):(e=t.normal.x*this.max.x,n=t.normal.x*this.min.x),t.normal.y>0?(e+=t.normal.y*this.min.y,n+=t.normal.y*this.max.y):(e+=t.normal.y*this.max.y,n+=t.normal.y*this.min.y),t.normal.z>0?(e+=t.normal.z*this.min.z,n+=t.normal.z*this.max.z):(e+=t.normal.z*this.max.z,n+=t.normal.z*this.min.z),e<=-t.constant&&n>=-t.constant}intersectsTriangle(t){if(this.isEmpty())return!1;this.getCenter(bi),Ji.subVectors(this.max,bi),ei.subVectors(t.a,bi),ni.subVectors(t.b,bi),ii.subVectors(t.c,bi),Rn.subVectors(ni,ei),Cn.subVectors(ii,ni),Fn.subVectors(ei,ii);let e=[0,-Rn.z,Rn.y,0,-Cn.z,Cn.y,0,-Fn.z,Fn.y,Rn.z,0,-Rn.x,Cn.z,0,-Cn.x,Fn.z,0,-Fn.x,-Rn.y,Rn.x,0,-Cn.y,Cn.x,0,-Fn.y,Fn.x,0];return!ls(e,ei,ni,ii,Ji)||(e=[1,0,0,0,1,0,0,0,1],!ls(e,ei,ni,ii,Ji))?!1:(Qi.crossVectors(Rn,Cn),e=[Qi.x,Qi.y,Qi.z],ls(e,ei,ni,ii,Ji))}clampPoint(t,e){return e.copy(t).clamp(this.min,this.max)}distanceToPoint(t){return this.clampPoint(t,Ye).distanceTo(t)}getBoundingSphere(t){return this.isEmpty()?t.makeEmpty():(this.getCenter(t.center),t.radius=this.getSize(Ye).length()*.5),t}intersect(t){return this.min.max(t.min),this.max.min(t.max),this.isEmpty()&&this.makeEmpty(),this}union(t){return this.min.min(t.min),this.max.max(t.max),this}applyMatrix4(t){return this.isEmpty()?this:(pn[0].set(this.min.x,this.min.y,this.min.z).applyMatrix4(t),pn[1].set(this.min.x,this.min.y,this.max.z).applyMatrix4(t),pn[2].set(this.min.x,this.max.y,this.min.z).applyMatrix4(t),pn[3].set(this.min.x,this.max.y,this.max.z).applyMatrix4(t),pn[4].set(this.max.x,this.min.y,this.min.z).applyMatrix4(t),pn[5].set(this.max.x,this.min.y,this.max.z).applyMatrix4(t),pn[6].set(this.max.x,this.max.y,this.min.z).applyMatrix4(t),pn[7].set(this.max.x,this.max.y,this.max.z).applyMatrix4(t),this.setFromPoints(pn),this)}translate(t){return this.min.add(t),this.max.add(t),this}equals(t){return t.min.equals(this.min)&&t.max.equals(this.max)}toJSON(){return{min:this.min.toArray(),max:this.max.toArray()}}fromJSON(t){return this.min.fromArray(t.min),this.max.fromArray(t.max),this}}const pn=[new I,new I,new I,new I,new I,new I,new I,new I],Ye=new I,$i=new Yn,ei=new I,ni=new I,ii=new I,Rn=new I,Cn=new I,Fn=new I,bi=new I,Ji=new I,Qi=new I,On=new I;function ls(i,t,e,n,r){for(let s=0,a=i.length-3;s<=a;s+=3){On.fromArray(i,s);const o=r.x*Math.abs(On.x)+r.y*Math.abs(On.y)+r.z*Math.abs(On.z),c=t.dot(On),l=e.dot(On),d=n.dot(On);if(Math.max(-Math.max(c,l,d),Math.min(c,l,d))>o)return!1}return!0}const fe=new I,ji=new Ct;let ch=0;class Qe extends Un{constructor(t,e,n=!1){if(super(),Array.isArray(t))throw new TypeError("THREE.BufferAttribute: array should be a Typed Array.");this.isBufferAttribute=!0,Object.defineProperty(this,"id",{value:ch++}),this.name="",this.array=t,this.itemSize=e,this.count=t!==void 0?t.length/e:0,this.normalized=n,this.usage=vl,this.updateRanges=[],this.gpuType=Ke,this.version=0}onUploadCallback(){}set needsUpdate(t){t===!0&&this.version++}setUsage(t){return this.usage=t,this}addUpdateRange(t,e){this.updateRanges.push({start:t,count:e})}clearUpdateRanges(){this.updateRanges.length=0}copy(t){return this.name=t.name,this.array=new t.array.constructor(t.array),this.itemSize=t.itemSize,this.count=t.count,this.normalized=t.normalized,this.usage=t.usage,this.gpuType=t.gpuType,this}copyAt(t,e,n){t*=this.itemSize,n*=e.itemSize;for(let r=0,s=this.itemSize;r<s;r++)this.array[t+r]=e.array[n+r];return this}copyArray(t){return this.array.set(t),this}applyMatrix3(t){if(this.itemSize===2)for(let e=0,n=this.count;e<n;e++)ji.fromBufferAttribute(this,e),ji.applyMatrix3(t),this.setXY(e,ji.x,ji.y);else if(this.itemSize===3)for(let e=0,n=this.count;e<n;e++)fe.fromBufferAttribute(this,e),fe.applyMatrix3(t),this.setXYZ(e,fe.x,fe.y,fe.z);return this}applyMatrix4(t){for(let e=0,n=this.count;e<n;e++)fe.fromBufferAttribute(this,e),fe.applyMatrix4(t),this.setXYZ(e,fe.x,fe.y,fe.z);return this}applyNormalMatrix(t){for(let e=0,n=this.count;e<n;e++)fe.fromBufferAttribute(this,e),fe.applyNormalMatrix(t),this.setXYZ(e,fe.x,fe.y,fe.z);return this}transformDirection(t){for(let e=0,n=this.count;e<n;e++)fe.fromBufferAttribute(this,e),fe.transformDirection(t),this.setXYZ(e,fe.x,fe.y,fe.z);return this}set(t,e=0){return this.array.set(t,e),this}getComponent(t,e){let n=this.array[t*this.itemSize+e];return this.normalized&&(n=Ze(n,this.array)),n}setComponent(t,e,n){return this.normalized&&(n=Jt(n,this.array)),this.array[t*this.itemSize+e]=n,this}getX(t){let e=this.array[t*this.itemSize];return this.normalized&&(e=Ze(e,this.array)),e}setX(t,e){return this.normalized&&(e=Jt(e,this.array)),this.array[t*this.itemSize]=e,this}getY(t){let e=this.array[t*this.itemSize+1];return this.normalized&&(e=Ze(e,this.array)),e}setY(t,e){return this.normalized&&(e=Jt(e,this.array)),this.array[t*this.itemSize+1]=e,this}getZ(t){let e=this.array[t*this.itemSize+2];return this.normalized&&(e=Ze(e,this.array)),e}setZ(t,e){return this.normalized&&(e=Jt(e,this.array)),this.array[t*this.itemSize+2]=e,this}getW(t){let e=this.array[t*this.itemSize+3];return this.normalized&&(e=Ze(e,this.array)),e}setW(t,e){return this.normalized&&(e=Jt(e,this.array)),this.array[t*this.itemSize+3]=e,this}setXY(t,e,n){return t*=this.itemSize,this.normalized&&(e=Jt(e,this.array),n=Jt(n,this.array)),this.array[t+0]=e,this.array[t+1]=n,this}setXYZ(t,e,n,r){return t*=this.itemSize,this.normalized&&(e=Jt(e,this.array),n=Jt(n,this.array),r=Jt(r,this.array)),this.array[t+0]=e,this.array[t+1]=n,this.array[t+2]=r,this}setXYZW(t,e,n,r,s){return t*=this.itemSize,this.normalized&&(e=Jt(e,this.array),n=Jt(n,this.array),r=Jt(r,this.array),s=Jt(s,this.array)),this.array[t+0]=e,this.array[t+1]=n,this.array[t+2]=r,this.array[t+3]=s,this}onUpload(t){return this.onUploadCallback=t,this}clone(){return new this.constructor(this.array,this.itemSize).copy(this)}toJSON(){const t={itemSize:this.itemSize,type:this.array.constructor.name,array:Array.from(this.array),normalized:this.normalized};return t.name=this.name,t.usage=this.usage,t.gpuType=this.gpuType,t}dispose(){this.dispatchEvent({type:"dispose"})}}class bl extends Qe{constructor(t,e,n){super(new Uint16Array(t),e,n)}}class Tl extends Qe{constructor(t,e,n){super(new Uint32Array(t),e,n)}}class we extends Qe{constructor(t,e,n){super(new Float32Array(t),e,n)}}const hh=new Yn,Ti=new I,cs=new I;class Si{constructor(t=new I,e=-1){this.isSphere=!0,this.center=t,this.radius=e}set(t,e){return this.center.copy(t),this.radius=e,this}setFromPoints(t,e){const n=this.center;e!==void 0?n.copy(e):hh.setFromPoints(t).getCenter(n);let r=0;for(let s=0,a=t.length;s<a;s++)r=Math.max(r,n.distanceToSquared(t[s]));return this.radius=Math.sqrt(r),this}copy(t){return this.center.copy(t.center),this.radius=t.radius,this}isEmpty(){return this.radius<0}makeEmpty(){return this.center.set(0,0,0),this.radius=-1,this}containsPoint(t){return t.distanceToSquared(this.center)<=this.radius*this.radius}distanceToPoint(t){return t.distanceTo(this.center)-this.radius}intersectsSphere(t){const e=this.radius+t.radius;return t.center.distanceToSquared(this.center)<=e*e}intersectsBox(t){return t.intersectsSphere(this)}intersectsPlane(t){return Math.abs(t.distanceToPoint(this.center))<=this.radius}clampPoint(t,e){const n=this.center.distanceToSquared(t);return e.copy(t),n>this.radius*this.radius&&(e.sub(this.center).normalize(),e.multiplyScalar(this.radius).add(this.center)),e}getBoundingBox(t){return this.isEmpty()?(t.makeEmpty(),t):(t.set(this.center,this.center),t.expandByScalar(this.radius),t)}applyMatrix4(t){return this.center.applyMatrix4(t),this.radius=this.radius*t.getMaxScaleOnAxis(),this}translate(t){return this.center.add(t),this}expandByPoint(t){if(this.isEmpty())return this.center.copy(t),this.radius=0,this;Ti.subVectors(t,this.center);const e=Ti.lengthSq();if(e>this.radius*this.radius){const n=Math.sqrt(e),r=(n-this.radius)*.5;this.center.addScaledVector(Ti,r/n),this.radius+=r}return this}union(t){return t.isEmpty()?this:this.isEmpty()?(this.copy(t),this):(this.center.equals(t.center)===!0?this.radius=Math.max(this.radius,t.radius):(cs.subVectors(t.center,this.center).setLength(t.radius),this.expandByPoint(Ti.copy(t.center).add(cs)),this.expandByPoint(Ti.copy(t.center).sub(cs))),this)}equals(t){return t.center.equals(this.center)&&t.radius===this.radius}clone(){return new this.constructor().copy(this)}toJSON(){return{radius:this.radius,center:this.center.toArray()}}fromJSON(t){return this.radius=t.radius,this.center.fromArray(t.center),this}}let uh=0;const Ge=new ee,hs=new Se,ri=new I,Oe=new Yn,Ai=new Yn,ve=new I;class Ne extends Un{constructor(){super(),this.isBufferGeometry=!0,Object.defineProperty(this,"id",{value:uh++}),this.uuid=Sn(),this.name="",this.type="BufferGeometry",this.index=null,this.indirect=null,this.indirectOffset=0,this.attributes={},this.morphAttributes={},this.morphTargetsRelative=!1,this.groups=[],this.boundingBox=null,this.boundingSphere=null,this.drawRange={start:0,count:1/0},this.userData={},this._transformed=!1}getIndex(){return this.index}setIndex(t){return Array.isArray(t)?this.index=new(Cc(t)?Tl:bl)(t,1):this.index=t,this}setIndirect(t,e=0){return this.indirect=t,this.indirectOffset=e,this}getIndirect(){return this.indirect}getAttribute(t){return this.attributes[t]}setAttribute(t,e){return this.attributes[t]=e,this}deleteAttribute(t){return delete this.attributes[t],this}hasAttribute(t){return this.attributes[t]!==void 0}addGroup(t,e,n=0){this.groups.push({start:t,count:e,materialIndex:n})}clearGroups(){this.groups=[]}setDrawRange(t,e){this.drawRange.start=t,this.drawRange.count=e}applyMatrix4(t){const e=this.attributes.position;e!==void 0&&(e.applyMatrix4(t),e.needsUpdate=!0);const n=this.attributes.normal;if(n!==void 0){const s=new Dt().getNormalMatrix(t);n.applyNormalMatrix(s),n.needsUpdate=!0}const r=this.attributes.tangent;return r!==void 0&&(r.transformDirection(t),r.needsUpdate=!0),this.boundingBox!==null&&this.computeBoundingBox(),this.boundingSphere!==null&&this.computeBoundingSphere(),this._transformed=!0,this}applyQuaternion(t){return Ge.makeRotationFromQuaternion(t),this.applyMatrix4(Ge),this}rotateX(t){return Ge.makeRotationX(t),this.applyMatrix4(Ge),this}rotateY(t){return Ge.makeRotationY(t),this.applyMatrix4(Ge),this}rotateZ(t){return Ge.makeRotationZ(t),this.applyMatrix4(Ge),this}translate(t,e,n){return Ge.makeTranslation(t,e,n),this.applyMatrix4(Ge),this}scale(t,e,n){return Ge.makeScale(t,e,n),this.applyMatrix4(Ge),this}lookAt(t){return hs.lookAt(t),hs.updateMatrix(),this.applyMatrix4(hs.matrix),this}center(){return this.computeBoundingBox(),this.boundingBox.getCenter(ri).negate(),this.translate(ri.x,ri.y,ri.z),this}setFromPoints(t){const e=this.getAttribute("position");if(e===void 0){const n=[];for(let r=0,s=t.length;r<s;r++){const a=t[r];n.push(a.x,a.y,a.z||0)}this.setAttribute("position",new we(n,3))}else{const n=Math.min(t.length,e.count);for(let r=0;r<n;r++){const s=t[r];e.setXYZ(r,s.x,s.y,s.z||0)}t.length>e.count&&Rt("BufferGeometry: Buffer size too small for points data. Use .dispose() and create a new geometry."),e.needsUpdate=!0}return this}computeBoundingBox(){this.boundingBox===null&&(this.boundingBox=new Yn);const t=this.attributes.position,e=this.morphAttributes.position;if(t&&t.isGLBufferAttribute){Wt("BufferGeometry.computeBoundingBox(): GLBufferAttribute requires a manual bounding box.",this),this.boundingBox.set(new I(-1/0,-1/0,-1/0),new I(1/0,1/0,1/0));return}if(t!==void 0){if(this.boundingBox.setFromBufferAttribute(t),e)for(let n=0,r=e.length;n<r;n++){const s=e[n];Oe.setFromBufferAttribute(s),this.morphTargetsRelative?(ve.addVectors(this.boundingBox.min,Oe.min),this.boundingBox.expandByPoint(ve),ve.addVectors(this.boundingBox.max,Oe.max),this.boundingBox.expandByPoint(ve)):(this.boundingBox.expandByPoint(Oe.min),this.boundingBox.expandByPoint(Oe.max))}}else this.boundingBox.makeEmpty();(isNaN(this.boundingBox.min.x)||isNaN(this.boundingBox.min.y)||isNaN(this.boundingBox.min.z))&&Wt('BufferGeometry.computeBoundingBox(): Computed min/max have NaN values. The "position" attribute is likely to have NaN values.',this)}computeBoundingSphere(){this.boundingSphere===null&&(this.boundingSphere=new Si);const t=this.attributes.position,e=this.morphAttributes.position;if(t&&t.isGLBufferAttribute){Wt("BufferGeometry.computeBoundingSphere(): GLBufferAttribute requires a manual bounding sphere.",this),this.boundingSphere.set(new I,1/0);return}if(t){const n=this.boundingSphere.center;if(Oe.setFromBufferAttribute(t),e)for(let s=0,a=e.length;s<a;s++){const o=e[s];Ai.setFromBufferAttribute(o),this.morphTargetsRelative?(ve.addVectors(Oe.min,Ai.min),Oe.expandByPoint(ve),ve.addVectors(Oe.max,Ai.max),Oe.expandByPoint(ve)):(Oe.expandByPoint(Ai.min),Oe.expandByPoint(Ai.max))}Oe.getCenter(n);let r=0;for(let s=0,a=t.count;s<a;s++)ve.fromBufferAttribute(t,s),r=Math.max(r,n.distanceToSquared(ve));if(e)for(let s=0,a=e.length;s<a;s++){const o=e[s],c=this.morphTargetsRelative;for(let l=0,d=o.count;l<d;l++)ve.fromBufferAttribute(o,l),c&&(ri.fromBufferAttribute(t,l),ve.add(ri)),r=Math.max(r,n.distanceToSquared(ve))}this.boundingSphere.radius=Math.sqrt(r),isNaN(this.boundingSphere.radius)&&Wt('BufferGeometry.computeBoundingSphere(): Computed radius is NaN. The "position" attribute is likely to have NaN values.',this)}}computeTangents(){const t=this.index,e=this.attributes;if(t===null||e.position===void 0||e.normal===void 0||e.uv===void 0){Wt("BufferGeometry: .computeTangents() failed. Missing required attributes (index, position, normal or uv)");return}const n=e.position,r=e.normal,s=e.uv;let a=this.getAttribute("tangent");(a===void 0||a.count!==n.count)&&(a=new Qe(new Float32Array(4*n.count),4),this.setAttribute("tangent",a));const o=[],c=[];for(let x=0;x<n.count;x++)o[x]=new I,c[x]=new I;const l=new I,d=new I,p=new I,h=new Ct,_=new Ct,v=new Ct,M=new I,m=new I;function u(x,T,L){l.fromBufferAttribute(n,x),d.fromBufferAttribute(n,T),p.fromBufferAttribute(n,L),h.fromBufferAttribute(s,x),_.fromBufferAttribute(s,T),v.fromBufferAttribute(s,L),d.sub(l),p.sub(l),_.sub(h),v.sub(h);const F=1/(_.x*v.y-v.x*_.y);isFinite(F)&&(M.copy(d).multiplyScalar(v.y).addScaledVector(p,-_.y).multiplyScalar(F),m.copy(p).multiplyScalar(_.x).addScaledVector(d,-v.x).multiplyScalar(F),o[x].add(M),o[T].add(M),o[L].add(M),c[x].add(m),c[T].add(m),c[L].add(m))}let A=this.groups;A.length===0&&(A=[{start:0,count:t.count}]);for(let x=0,T=A.length;x<T;++x){const L=A[x],F=L.start,G=L.count;for(let k=F,U=F+G;k<U;k+=3)u(t.getX(k+0),t.getX(k+1),t.getX(k+2))}const P=new I,S=new I,b=new I,y=new I;function w(x){b.fromBufferAttribute(r,x),y.copy(b);const T=o[x];P.copy(T),P.sub(b.multiplyScalar(b.dot(T))).normalize(),S.crossVectors(y,T);const F=S.dot(c[x])<0?-1:1;a.setXYZW(x,P.x,P.y,P.z,F)}for(let x=0,T=A.length;x<T;++x){const L=A[x],F=L.start,G=L.count;for(let k=F,U=F+G;k<U;k+=3)w(t.getX(k+0)),w(t.getX(k+1)),w(t.getX(k+2))}this._transformed=!0}computeVertexNormals(){const t=this.index,e=this.getAttribute("position");if(e!==void 0){let n=this.getAttribute("normal");if(n===void 0||n.count!==e.count)n=new Qe(new Float32Array(e.count*3),3),this.setAttribute("normal",n);else for(let h=0,_=n.count;h<_;h++)n.setXYZ(h,0,0,0);const r=new I,s=new I,a=new I,o=new I,c=new I,l=new I,d=new I,p=new I;if(t)for(let h=0,_=t.count;h<_;h+=3){const v=t.getX(h+0),M=t.getX(h+1),m=t.getX(h+2);r.fromBufferAttribute(e,v),s.fromBufferAttribute(e,M),a.fromBufferAttribute(e,m),d.subVectors(a,s),p.subVectors(r,s),d.cross(p),o.fromBufferAttribute(n,v),c.fromBufferAttribute(n,M),l.fromBufferAttribute(n,m),o.add(d),c.add(d),l.add(d),n.setXYZ(v,o.x,o.y,o.z),n.setXYZ(M,c.x,c.y,c.z),n.setXYZ(m,l.x,l.y,l.z)}else for(let h=0,_=e.count;h<_;h+=3)r.fromBufferAttribute(e,h+0),s.fromBufferAttribute(e,h+1),a.fromBufferAttribute(e,h+2),d.subVectors(a,s),p.subVectors(r,s),d.cross(p),n.setXYZ(h+0,d.x,d.y,d.z),n.setXYZ(h+1,d.x,d.y,d.z),n.setXYZ(h+2,d.x,d.y,d.z);this.normalizeNormals(),n.needsUpdate=!0}}normalizeNormals(){const t=this.attributes.normal;for(let e=0,n=t.count;e<n;e++)ve.fromBufferAttribute(t,e),ve.normalize(),t.setXYZ(e,ve.x,ve.y,ve.z)}toNonIndexed(){function t(o,c){const l=o.array,d=o.itemSize,p=o.normalized,h=new l.constructor(c.length*d);let _=0,v=0;for(let M=0,m=c.length;M<m;M++){o.isInterleavedBufferAttribute?_=c[M]*o.data.stride+o.offset:_=c[M]*d;for(let u=0;u<d;u++)h[v++]=l[_++]}return new Qe(h,d,p)}if(this.index===null)return Rt("BufferGeometry.toNonIndexed(): BufferGeometry is already non-indexed."),this;const e=new Ne,n=this.index.array,r=this.attributes;for(const o in r){const c=r[o],l=t(c,n);e.setAttribute(o,l)}const s=this.morphAttributes;for(const o in s){const c=[],l=s[o];for(let d=0,p=l.length;d<p;d++){const h=l[d],_=t(h,n);c.push(_)}e.morphAttributes[o]=c}e.morphTargetsRelative=this.morphTargetsRelative;const a=this.groups;for(let o=0,c=a.length;o<c;o++){const l=a[o];e.addGroup(l.start,l.count,l.materialIndex)}return e}toJSON(){const t={metadata:{version:4.7,type:"BufferGeometry",generator:"BufferGeometry.toJSON"}};if(t.uuid=this.uuid,t.type=this.parameters!==void 0&&this._transformed===!0?"BufferGeometry":this.type,t.name=this.name,Object.keys(this.userData).length>0&&(t.userData=this.userData),this.parameters!==void 0&&this._transformed!==!0){const c=this.parameters;for(const l in c)c[l]!==void 0&&(t[l]=c[l]);return t}t.data={attributes:{}};const e=this.index;e!==null&&(t.data.index={type:e.array.constructor.name,array:Array.prototype.slice.call(e.array)});const n=this.attributes;for(const c in n){const l=n[c];t.data.attributes[c]=l.toJSON(t.data)}const r={};let s=!1;for(const c in this.morphAttributes){const l=this.morphAttributes[c],d=[];for(let p=0,h=l.length;p<h;p++){const _=l[p];d.push(_.toJSON(t.data))}d.length>0&&(r[c]=d,s=!0)}s&&(t.data.morphAttributes=r,t.data.morphTargetsRelative=this.morphTargetsRelative);const a=this.groups;a.length>0&&(t.data.groups=JSON.parse(JSON.stringify(a)));const o=this.boundingSphere;return o!==null&&(t.data.boundingSphere=o.toJSON()),t}clone(){return new this.constructor().copy(this)}copy(t){this.index=null,this.attributes={},this.morphAttributes={},this.groups=[],this.boundingBox=null,this.boundingSphere=null;const e={};this.name=t.name;const n=t.index;n!==null&&this.setIndex(n.clone());const r=t.attributes;for(const l in r){const d=r[l];this.setAttribute(l,d.clone(e))}const s=t.morphAttributes;for(const l in s){const d=[],p=s[l];for(let h=0,_=p.length;h<_;h++)d.push(p[h].clone(e));this.morphAttributes[l]=d}this.morphTargetsRelative=t.morphTargetsRelative;const a=t.groups;for(let l=0,d=a.length;l<d;l++){const p=a[l];this.addGroup(p.start,p.count,p.materialIndex)}const o=t.boundingBox;o!==null&&(this.boundingBox=o.clone());const c=t.boundingSphere;return c!==null&&(this.boundingSphere=c.clone()),this.drawRange.start=t.drawRange.start,this.drawRange.count=t.drawRange.count,this.userData=t.userData,this._transformed=t._transformed,this}dispose(){this.dispatchEvent({type:"dispose"})}}class dh{constructor(t,e){this.isInterleavedBuffer=!0,this.array=t,this.stride=e,this.count=t!==void 0?t.length/e:0,this.usage=vl,this.updateRanges=[],this.version=0,this.uuid=Sn()}onUploadCallback(){}set needsUpdate(t){t===!0&&this.version++}setUsage(t){return this.usage=t,this}addUpdateRange(t,e){this.updateRanges.push({start:t,count:e})}clearUpdateRanges(){this.updateRanges.length=0}copy(t){return this.array=new t.array.constructor(t.array),this.count=t.count,this.stride=t.stride,this.usage=t.usage,this}copyAt(t,e,n){t*=this.stride,n*=e.stride;for(let r=0,s=this.stride;r<s;r++)this.array[t+r]=e.array[n+r];return this}set(t,e=0){return this.array.set(t,e),this}clone(t){t.arrayBuffers===void 0&&(t.arrayBuffers={}),this.array.buffer._uuid===void 0&&(this.array.buffer._uuid=Sn()),t.arrayBuffers[this.array.buffer._uuid]===void 0&&(t.arrayBuffers[this.array.buffer._uuid]=this.array.slice(0).buffer);const e=new this.array.constructor(t.arrayBuffers[this.array.buffer._uuid]),n=new this.constructor(e,this.stride);return n.setUsage(this.usage),n}onUpload(t){return this.onUploadCallback=t,this}toJSON(t){t.arrayBuffers===void 0&&(t.arrayBuffers={}),this.array.buffer._uuid===void 0&&(this.array.buffer._uuid=Sn()),t.arrayBuffers[this.array.buffer._uuid]===void 0&&(t.arrayBuffers[this.array.buffer._uuid]=Array.from(new Uint32Array(this.array.buffer)));const e={uuid:this.uuid,buffer:this.array.buffer._uuid,type:this.array.constructor.name,stride:this.stride};return e.usage=this.usage,e}}const Ce=new I;class Al{constructor(t,e,n,r=!1){this.isInterleavedBufferAttribute=!0,this.name="",this.data=t,this.itemSize=e,this.offset=n,this.normalized=r}get count(){return this.data.count}get array(){return this.data.array}set needsUpdate(t){this.data.needsUpdate=t}applyMatrix4(t){for(let e=0,n=this.data.count;e<n;e++)Ce.fromBufferAttribute(this,e),Ce.applyMatrix4(t),this.setXYZ(e,Ce.x,Ce.y,Ce.z);return this}applyNormalMatrix(t){for(let e=0,n=this.count;e<n;e++)Ce.fromBufferAttribute(this,e),Ce.applyNormalMatrix(t),this.setXYZ(e,Ce.x,Ce.y,Ce.z);return this}transformDirection(t){for(let e=0,n=this.count;e<n;e++)Ce.fromBufferAttribute(this,e),Ce.transformDirection(t),this.setXYZ(e,Ce.x,Ce.y,Ce.z);return this}getComponent(t,e){let n=this.array[t*this.data.stride+this.offset+e];return this.normalized&&(n=Ze(n,this.array)),n}setComponent(t,e,n){return this.normalized&&(n=Jt(n,this.array)),this.data.array[t*this.data.stride+this.offset+e]=n,this}setX(t,e){return this.normalized&&(e=Jt(e,this.array)),this.data.array[t*this.data.stride+this.offset]=e,this}setY(t,e){return this.normalized&&(e=Jt(e,this.array)),this.data.array[t*this.data.stride+this.offset+1]=e,this}setZ(t,e){return this.normalized&&(e=Jt(e,this.array)),this.data.array[t*this.data.stride+this.offset+2]=e,this}setW(t,e){return this.normalized&&(e=Jt(e,this.array)),this.data.array[t*this.data.stride+this.offset+3]=e,this}getX(t){let e=this.data.array[t*this.data.stride+this.offset];return this.normalized&&(e=Ze(e,this.array)),e}getY(t){let e=this.data.array[t*this.data.stride+this.offset+1];return this.normalized&&(e=Ze(e,this.array)),e}getZ(t){let e=this.data.array[t*this.data.stride+this.offset+2];return this.normalized&&(e=Ze(e,this.array)),e}getW(t){let e=this.data.array[t*this.data.stride+this.offset+3];return this.normalized&&(e=Ze(e,this.array)),e}setXY(t,e,n){return t=t*this.data.stride+this.offset,this.normalized&&(e=Jt(e,this.array),n=Jt(n,this.array)),this.data.array[t+0]=e,this.data.array[t+1]=n,this}setXYZ(t,e,n,r){return t=t*this.data.stride+this.offset,this.normalized&&(e=Jt(e,this.array),n=Jt(n,this.array),r=Jt(r,this.array)),this.data.array[t+0]=e,this.data.array[t+1]=n,this.data.array[t+2]=r,this}setXYZW(t,e,n,r,s){return t=t*this.data.stride+this.offset,this.normalized&&(e=Jt(e,this.array),n=Jt(n,this.array),r=Jt(r,this.array),s=Jt(s,this.array)),this.data.array[t+0]=e,this.data.array[t+1]=n,this.data.array[t+2]=r,this.data.array[t+3]=s,this}clone(t){if(t===void 0){Ir("InterleavedBufferAttribute.clone(): Cloning an interleaved buffer attribute will de-interleave buffer data.");const e=[];for(let n=0;n<this.count;n++){const r=n*this.data.stride+this.offset;for(let s=0;s<this.itemSize;s++)e.push(this.data.array[r+s])}return new Qe(new this.array.constructor(e),this.itemSize,this.normalized)}else return t.interleavedBuffers===void 0&&(t.interleavedBuffers={}),t.interleavedBuffers[this.data.uuid]===void 0&&(t.interleavedBuffers[this.data.uuid]=this.data.clone(t)),new Al(t.interleavedBuffers[this.data.uuid],this.itemSize,this.offset,this.normalized)}toJSON(t){if(t===void 0){Ir("InterleavedBufferAttribute.toJSON(): Serializing an interleaved buffer attribute will de-interleave buffer data.");const e=[];for(let n=0;n<this.count;n++){const r=n*this.data.stride+this.offset;for(let s=0;s<this.itemSize;s++)e.push(this.data.array[r+s])}return{itemSize:this.itemSize,type:this.array.constructor.name,array:e,normalized:this.normalized}}else return t.interleavedBuffers===void 0&&(t.interleavedBuffers={}),t.interleavedBuffers[this.data.uuid]===void 0&&(t.interleavedBuffers[this.data.uuid]=this.data.toJSON(t)),{isInterleavedBufferAttribute:!0,itemSize:this.itemSize,data:this.data.uuid,offset:this.offset,normalized:this.normalized}}}const us=new I,fh=new I,ph=new Dt;class _n{constructor(t=new I(1,0,0),e=0){this.isPlane=!0,this.normal=t,this.constant=e}set(t,e){return this.normal.copy(t),this.constant=e,this}setComponents(t,e,n,r){return this.normal.set(t,e,n),this.constant=r,this}setFromNormalAndCoplanarPoint(t,e){return this.normal.copy(t),this.constant=-e.dot(this.normal),this}setFromCoplanarPoints(t,e,n){const r=us.subVectors(n,e).cross(fh.subVectors(t,e)).normalize();return this.setFromNormalAndCoplanarPoint(r,t),this}copy(t){return this.normal.copy(t.normal),this.constant=t.constant,this}normalize(){const t=1/this.normal.length();return this.normal.multiplyScalar(t),this.constant*=t,this}negate(){return this.constant*=-1,this.normal.negate(),this}distanceToPoint(t){return this.normal.dot(t)+this.constant}distanceToSphere(t){return this.distanceToPoint(t.center)-t.radius}projectPoint(t,e){return e.copy(t).addScaledVector(this.normal,-this.distanceToPoint(t))}intersectLine(t,e,n=!0){const r=t.delta(us),s=this.normal.dot(r);if(s===0)return this.distanceToPoint(t.start)===0?e.copy(t.start):null;const a=-(t.start.dot(this.normal)+this.constant)/s;return n===!0&&(a<0||a>1)?null:e.copy(t.start).addScaledVector(r,a)}intersectsLine(t){const e=this.distanceToPoint(t.start),n=this.distanceToPoint(t.end);return e<0&&n>0||n<0&&e>0}intersectsBox(t){return t.intersectsPlane(this)}intersectsSphere(t){return t.intersectsPlane(this)}coplanarPoint(t){return t.copy(this.normal).multiplyScalar(-this.constant)}applyMatrix4(t,e){const n=e||ph.getNormalMatrix(t),r=this.coplanarPoint(us).applyMatrix4(t),s=this.normal.applyMatrix3(n).normalize();return this.constant=-r.dot(s),this}translate(t){return this.constant-=t.dot(this.normal),this}equals(t){return t.normal.equals(this.normal)&&t.constant===this.constant}clone(){return new this.constructor().copy(this)}toJSON(){return{normal:this.normal.toArray(),constant:this.constant}}fromJSON(t){return this.normal.fromArray(t.normal),this.constant=t.constant,this}}let mh=0;class Mi extends Un{constructor(){super(),this.isMaterial=!0,Object.defineProperty(this,"id",{value:mh++}),this.uuid=Sn(),this.name="",this.type="Material",this.blending=Ui,this.side=kn,this.vertexColors=!1,this.opacity=1,this.transparent=!1,this.alphaHash=!1,this.blendSrc=nl,this.blendDst=il,this.blendEquation=ui,this.blendSrcAlpha=null,this.blendDstAlpha=null,this.blendEquationAlpha=null,this.blendColor=new Gt(0,0,0),this.blendAlpha=0,this.depthFunc=Fi,this.depthTest=!0,this.depthWrite=!0,this.stencilWriteMask=255,this.stencilFunc=Ec,this.stencilRef=0,this.stencilFuncMask=255,this.stencilFail=qr,this.stencilZFail=qr,this.stencilZPass=qr,this.stencilWrite=!1,this.clippingPlanes=null,this.clipIntersection=!1,this.clipShadows=!1,this.shadowSide=null,this.colorWrite=!0,this.precision=null,this.polygonOffset=!1,this.polygonOffsetFactor=0,this.polygonOffsetUnits=0,this.dithering=!1,this.alphaToCoverage=!1,this.premultipliedAlpha=!1,this.forceSinglePass=!1,this.allowOverride=!0,this.visible=!0,this.toneMapped=!0,this.userData={},this.version=0,this._alphaTest=0}get alphaTest(){return this._alphaTest}set alphaTest(t){this._alphaTest>0!=t>0&&this.version++,this._alphaTest=t}onBeforeRender(){}onBeforeCompile(){}customProgramCacheKey(){return this.onBeforeCompile.toString()}setValues(t){if(t!==void 0)for(const e in t){const n=t[e];if(n===void 0){Rt(`Material: parameter '${e}' has value of undefined.`);continue}const r=this[e];if(r===void 0){Rt(`Material: '${e}' is not a property of THREE.${this.type}.`);continue}r&&r.isColor?r.set(n):r&&r.isVector2&&n&&n.isVector2||r&&r.isEuler&&n&&n.isEuler||r&&r.isVector3&&n&&n.isVector3?r.copy(n):this[e]=n}}toJSON(t){const e=t===void 0||typeof t=="string";e&&(t={textures:{},images:{}});const n={metadata:{version:4.7,type:"Material",generator:"Material.toJSON"}};n.uuid=this.uuid,n.type=this.type,n.blending=this.blending,n.side=this.side,n.shadowSide=this.shadowSide,n.vertexColors=this.vertexColors,n.opacity=this.opacity,n.transparent=this.transparent,n.blendSrc=this.blendSrc,n.blendDst=this.blendDst,n.blendEquation=this.blendEquation,n.blendSrcAlpha=this.blendSrcAlpha,n.blendDstAlpha=this.blendDstAlpha,n.blendEquationAlpha=this.blendEquationAlpha,n.blendColor=this.blendColor.getHex(),n.blendAlpha=this.blendAlpha,n.depthFunc=this.depthFunc,n.depthTest=this.depthTest,n.depthWrite=this.depthWrite,n.colorWrite=this.colorWrite,n.clipIntersection=this.clipIntersection,n.clipShadows=this.clipShadows,n.stencilWriteMask=this.stencilWriteMask,n.stencilFunc=this.stencilFunc,n.stencilRef=this.stencilRef,n.stencilFuncMask=this.stencilFuncMask,n.stencilFail=this.stencilFail,n.stencilZFail=this.stencilZFail,n.stencilZPass=this.stencilZPass,n.stencilWrite=this.stencilWrite,n.polygonOffset=this.polygonOffset,n.polygonOffsetFactor=this.polygonOffsetFactor,n.polygonOffsetUnits=this.polygonOffsetUnits,n.dithering=this.dithering,n.alphaTest=this.alphaTest,n.alphaHash=this.alphaHash,n.alphaToCoverage=this.alphaToCoverage,n.premultipliedAlpha=this.premultipliedAlpha,n.forceSinglePass=this.forceSinglePass,n.allowOverride=this.allowOverride,n.visible=this.visible,n.toneMapped=this.toneMapped,n.name=this.name,this.color&&this.color.isColor&&(n.color=this.color.getHex()),this.roughness!==void 0&&(n.roughness=this.roughness),this.metalness!==void 0&&(n.metalness=this.metalness),this.sheen!==void 0&&(n.sheen=this.sheen),this.sheenColor&&this.sheenColor.isColor&&(n.sheenColor=this.sheenColor.getHex()),this.sheenRoughness!==void 0&&(n.sheenRoughness=this.sheenRoughness),this.emissive&&this.emissive.isColor&&(n.emissive=this.emissive.getHex()),this.emissiveIntensity!==void 0&&(n.emissiveIntensity=this.emissiveIntensity),this.specular&&this.specular.isColor&&(n.specular=this.specular.getHex()),this.specularIntensity!==void 0&&(n.specularIntensity=this.specularIntensity),this.specularColor&&this.specularColor.isColor&&(n.specularColor=this.specularColor.getHex()),this.shininess!==void 0&&(n.shininess=this.shininess),this.clearcoat!==void 0&&(n.clearcoat=this.clearcoat),this.clearcoatRoughness!==void 0&&(n.clearcoatRoughness=this.clearcoatRoughness),this.clearcoatMap&&this.clearcoatMap.isTexture&&(n.clearcoatMap=this.clearcoatMap.toJSON(t).uuid),this.clearcoatRoughnessMap&&this.clearcoatRoughnessMap.isTexture&&(n.clearcoatRoughnessMap=this.clearcoatRoughnessMap.toJSON(t).uuid),this.clearcoatNormalMap&&this.clearcoatNormalMap.isTexture&&(n.clearcoatNormalMap=this.clearcoatNormalMap.toJSON(t).uuid,n.clearcoatNormalScale=this.clearcoatNormalScale.toArray()),this.sheenColorMap&&this.sheenColorMap.isTexture&&(n.sheenColorMap=this.sheenColorMap.toJSON(t).uuid),this.sheenRoughnessMap&&this.sheenRoughnessMap.isTexture&&(n.sheenRoughnessMap=this.sheenRoughnessMap.toJSON(t).uuid),this.dispersion!==void 0&&(n.dispersion=this.dispersion),this.retroreflectivity!==void 0&&(n.retroreflectivity=this.retroreflectivity),this.iridescence!==void 0&&(n.iridescence=this.iridescence),this.iridescenceIOR!==void 0&&(n.iridescenceIOR=this.iridescenceIOR),this.iridescenceThicknessRange!==void 0&&(n.iridescenceThicknessRange=this.iridescenceThicknessRange),this.iridescenceMap&&this.iridescenceMap.isTexture&&(n.iridescenceMap=this.iridescenceMap.toJSON(t).uuid),this.iridescenceThicknessMap&&this.iridescenceThicknessMap.isTexture&&(n.iridescenceThicknessMap=this.iridescenceThicknessMap.toJSON(t).uuid),this.anisotropy!==void 0&&(n.anisotropy=this.anisotropy),this.anisotropyRotation!==void 0&&(n.anisotropyRotation=this.anisotropyRotation),this.anisotropyMap&&this.anisotropyMap.isTexture&&(n.anisotropyMap=this.anisotropyMap.toJSON(t).uuid),this.map&&this.map.isTexture&&(n.map=this.map.toJSON(t).uuid),this.matcap&&this.matcap.isTexture&&(n.matcap=this.matcap.toJSON(t).uuid),this.alphaMap&&this.alphaMap.isTexture&&(n.alphaMap=this.alphaMap.toJSON(t).uuid),this.lightMap&&this.lightMap.isTexture&&(n.lightMap=this.lightMap.toJSON(t).uuid,n.lightMapIntensity=this.lightMapIntensity),this.aoMap&&this.aoMap.isTexture&&(n.aoMap=this.aoMap.toJSON(t).uuid,n.aoMapIntensity=this.aoMapIntensity),this.bumpMap&&this.bumpMap.isTexture&&(n.bumpMap=this.bumpMap.toJSON(t).uuid,n.bumpScale=this.bumpScale),this.normalMap&&this.normalMap.isTexture&&(n.normalMap=this.normalMap.toJSON(t).uuid,n.normalMapType=this.normalMapType,n.normalScale=this.normalScale.toArray()),this.displacementMap&&this.displacementMap.isTexture&&(n.displacementMap=this.displacementMap.toJSON(t).uuid,n.displacementScale=this.displacementScale,n.displacementBias=this.displacementBias),this.roughnessMap&&this.roughnessMap.isTexture&&(n.roughnessMap=this.roughnessMap.toJSON(t).uuid),this.metalnessMap&&this.metalnessMap.isTexture&&(n.metalnessMap=this.metalnessMap.toJSON(t).uuid),this.emissiveMap&&this.emissiveMap.isTexture&&(n.emissiveMap=this.emissiveMap.toJSON(t).uuid),this.specularMap&&this.specularMap.isTexture&&(n.specularMap=this.specularMap.toJSON(t).uuid),this.specularIntensityMap&&this.specularIntensityMap.isTexture&&(n.specularIntensityMap=this.specularIntensityMap.toJSON(t).uuid),this.specularColorMap&&this.specularColorMap.isTexture&&(n.specularColorMap=this.specularColorMap.toJSON(t).uuid),this.envMap&&this.envMap.isTexture&&(n.envMap=this.envMap.toJSON(t).uuid,this.combine!==void 0&&(n.combine=this.combine)),this.envMapRotation!==void 0&&(n.envMapRotation=this.envMapRotation.toArray()),this.envMapIntensity!==void 0&&(n.envMapIntensity=this.envMapIntensity),this.reflectivity!==void 0&&(n.reflectivity=this.reflectivity),this.refractionRatio!==void 0&&(n.refractionRatio=this.refractionRatio),this.gradientMap&&this.gradientMap.isTexture&&(n.gradientMap=this.gradientMap.toJSON(t).uuid),this.transmission!==void 0&&(n.transmission=this.transmission),this.transmissionMap&&this.transmissionMap.isTexture&&(n.transmissionMap=this.transmissionMap.toJSON(t).uuid),this.thickness!==void 0&&(n.thickness=this.thickness),this.thicknessMap&&this.thicknessMap.isTexture&&(n.thicknessMap=this.thicknessMap.toJSON(t).uuid),this.attenuationDistance!==void 0&&(n.attenuationDistance=this.attenuationDistance),this.attenuationColor!==void 0&&(n.attenuationColor=this.attenuationColor.getHex()),this.size!==void 0&&(n.size=this.size),this.sizeAttenuation!==void 0&&(n.sizeAttenuation=this.sizeAttenuation),Array.isArray(this.clippingPlanes)&&this.clippingPlanes.length>0&&(n.clippingPlanes=this.clippingPlanes.map(s=>s.toJSON())),this.rotation!==void 0&&(n.rotation=this.rotation),this.depthPacking!==void 0&&(n.depthPacking=this.depthPacking),this.linewidth!==void 0&&(n.linewidth=this.linewidth),this.linecap!==void 0&&(n.linecap=this.linecap),this.linejoin!==void 0&&(n.linejoin=this.linejoin),this.dashSize!==void 0&&(n.dashSize=this.dashSize),this.gapSize!==void 0&&(n.gapSize=this.gapSize),this.scale!==void 0&&(n.scale=this.scale),this.wireframe!==void 0&&(n.wireframe=this.wireframe),this.wireframeLinewidth!==void 0&&(n.wireframeLinewidth=this.wireframeLinewidth),this.wireframeLinecap!==void 0&&(n.wireframeLinecap=this.wireframeLinecap),this.wireframeLinejoin!==void 0&&(n.wireframeLinejoin=this.wireframeLinejoin),this.flatShading!==void 0&&(n.flatShading=this.flatShading),this.fog!==void 0&&(n.fog=this.fog),Object.keys(this.userData).length>0&&(n.userData=this.userData);function r(s){const a=[];for(const o in s){const c=s[o];delete c.metadata,a.push(c)}return a}if(e){const s=r(t.textures),a=r(t.images);s.length>0&&(n.textures=s),a.length>0&&(n.images=a)}return n}fromJSON(t,e){if(t.uuid!==void 0&&(this.uuid=t.uuid),t.name!==void 0&&(this.name=t.name),t.color!==void 0&&this.color!==void 0&&this.color.setHex(t.color),t.roughness!==void 0&&(this.roughness=t.roughness),t.metalness!==void 0&&(this.metalness=t.metalness),t.sheen!==void 0&&(this.sheen=t.sheen),t.sheenColor!==void 0&&(this.sheenColor=new Gt().setHex(t.sheenColor)),t.sheenRoughness!==void 0&&(this.sheenRoughness=t.sheenRoughness),t.emissive!==void 0&&this.emissive!==void 0&&this.emissive.setHex(t.emissive),t.specular!==void 0&&this.specular!==void 0&&this.specular.setHex(t.specular),t.specularIntensity!==void 0&&(this.specularIntensity=t.specularIntensity),t.specularColor!==void 0&&this.specularColor!==void 0&&this.specularColor.setHex(t.specularColor),t.shininess!==void 0&&(this.shininess=t.shininess),t.clearcoat!==void 0&&(this.clearcoat=t.clearcoat),t.clearcoatRoughness!==void 0&&(this.clearcoatRoughness=t.clearcoatRoughness),t.dispersion!==void 0&&(this.dispersion=t.dispersion),t.retroreflectivity!==void 0&&(this.retroreflectivity=t.retroreflectivity),t.iridescence!==void 0&&(this.iridescence=t.iridescence),t.iridescenceIOR!==void 0&&(this.iridescenceIOR=t.iridescenceIOR),t.iridescenceThicknessRange!==void 0&&(this.iridescenceThicknessRange=t.iridescenceThicknessRange),t.transmission!==void 0&&(this.transmission=t.transmission),t.thickness!==void 0&&(this.thickness=t.thickness),t.attenuationDistance!==void 0&&(this.attenuationDistance=t.attenuationDistance),t.attenuationColor!==void 0&&this.attenuationColor!==void 0&&this.attenuationColor.setHex(t.attenuationColor),t.anisotropy!==void 0&&(this.anisotropy=t.anisotropy),t.anisotropyRotation!==void 0&&(this.anisotropyRotation=t.anisotropyRotation),t.fog!==void 0&&(this.fog=t.fog),t.flatShading!==void 0&&(this.flatShading=t.flatShading),t.blending!==void 0&&(this.blending=t.blending),t.combine!==void 0&&(this.combine=t.combine),t.side!==void 0&&(this.side=t.side),t.shadowSide!==void 0&&(this.shadowSide=t.shadowSide),t.opacity!==void 0&&(this.opacity=t.opacity),t.transparent!==void 0&&(this.transparent=t.transparent),t.alphaTest!==void 0&&(this.alphaTest=t.alphaTest),t.alphaHash!==void 0&&(this.alphaHash=t.alphaHash),t.depthFunc!==void 0&&(this.depthFunc=t.depthFunc),t.depthTest!==void 0&&(this.depthTest=t.depthTest),t.depthWrite!==void 0&&(this.depthWrite=t.depthWrite),t.colorWrite!==void 0&&(this.colorWrite=t.colorWrite),t.clippingPlanes!==void 0&&(this.clippingPlanes=t.clippingPlanes.map(n=>new _n().fromJSON(n))),t.clipIntersection!==void 0&&(this.clipIntersection=t.clipIntersection),t.clipShadows!==void 0&&(this.clipShadows=t.clipShadows),t.depthPacking!==void 0&&(this.depthPacking=t.depthPacking),t.blendSrc!==void 0&&(this.blendSrc=t.blendSrc),t.blendDst!==void 0&&(this.blendDst=t.blendDst),t.blendEquation!==void 0&&(this.blendEquation=t.blendEquation),t.blendSrcAlpha!==void 0&&(this.blendSrcAlpha=t.blendSrcAlpha),t.blendDstAlpha!==void 0&&(this.blendDstAlpha=t.blendDstAlpha),t.blendEquationAlpha!==void 0&&(this.blendEquationAlpha=t.blendEquationAlpha),t.blendColor!==void 0&&this.blendColor!==void 0&&this.blendColor.setHex(t.blendColor),t.blendAlpha!==void 0&&(this.blendAlpha=t.blendAlpha),t.stencilWriteMask!==void 0&&(this.stencilWriteMask=t.stencilWriteMask),t.stencilFunc!==void 0&&(this.stencilFunc=t.stencilFunc),t.stencilRef!==void 0&&(this.stencilRef=t.stencilRef),t.stencilFuncMask!==void 0&&(this.stencilFuncMask=t.stencilFuncMask),t.stencilFail!==void 0&&(this.stencilFail=t.stencilFail),t.stencilZFail!==void 0&&(this.stencilZFail=t.stencilZFail),t.stencilZPass!==void 0&&(this.stencilZPass=t.stencilZPass),t.stencilWrite!==void 0&&(this.stencilWrite=t.stencilWrite),t.wireframe!==void 0&&(this.wireframe=t.wireframe),t.wireframeLinewidth!==void 0&&(this.wireframeLinewidth=t.wireframeLinewidth),t.wireframeLinecap!==void 0&&(this.wireframeLinecap=t.wireframeLinecap),t.wireframeLinejoin!==void 0&&(this.wireframeLinejoin=t.wireframeLinejoin),t.rotation!==void 0&&(this.rotation=t.rotation),t.linewidth!==void 0&&(this.linewidth=t.linewidth),t.linecap!==void 0&&(this.linecap=t.linecap),t.linejoin!==void 0&&(this.linejoin=t.linejoin),t.dashSize!==void 0&&(this.dashSize=t.dashSize),t.gapSize!==void 0&&(this.gapSize=t.gapSize),t.scale!==void 0&&(this.scale=t.scale),t.polygonOffset!==void 0&&(this.polygonOffset=t.polygonOffset),t.polygonOffsetFactor!==void 0&&(this.polygonOffsetFactor=t.polygonOffsetFactor),t.polygonOffsetUnits!==void 0&&(this.polygonOffsetUnits=t.polygonOffsetUnits),t.dithering!==void 0&&(this.dithering=t.dithering),t.alphaToCoverage!==void 0&&(this.alphaToCoverage=t.alphaToCoverage),t.premultipliedAlpha!==void 0&&(this.premultipliedAlpha=t.premultipliedAlpha),t.forceSinglePass!==void 0&&(this.forceSinglePass=t.forceSinglePass),t.allowOverride!==void 0&&(this.allowOverride=t.allowOverride),t.visible!==void 0&&(this.visible=t.visible),t.toneMapped!==void 0&&(this.toneMapped=t.toneMapped),t.userData!==void 0&&(this.userData=t.userData),t.vertexColors!==void 0&&(typeof t.vertexColors=="number"?this.vertexColors=t.vertexColors>0:this.vertexColors=t.vertexColors),t.size!==void 0&&(this.size=t.size),t.sizeAttenuation!==void 0&&(this.sizeAttenuation=t.sizeAttenuation),t.map!==void 0&&(this.map=e[t.map]||null),t.matcap!==void 0&&(this.matcap=e[t.matcap]||null),t.alphaMap!==void 0&&(this.alphaMap=e[t.alphaMap]||null),t.bumpMap!==void 0&&(this.bumpMap=e[t.bumpMap]||null),t.bumpScale!==void 0&&(this.bumpScale=t.bumpScale),t.normalMap!==void 0&&(this.normalMap=e[t.normalMap]||null),t.normalMapType!==void 0&&(this.normalMapType=t.normalMapType),t.normalScale!==void 0){let n=t.normalScale;Array.isArray(n)===!1&&(n=[n,n]),this.normalScale=new Ct().fromArray(n)}return t.displacementMap!==void 0&&(this.displacementMap=e[t.displacementMap]||null),t.displacementScale!==void 0&&(this.displacementScale=t.displacementScale),t.displacementBias!==void 0&&(this.displacementBias=t.displacementBias),t.roughnessMap!==void 0&&(this.roughnessMap=e[t.roughnessMap]||null),t.metalnessMap!==void 0&&(this.metalnessMap=e[t.metalnessMap]||null),t.emissiveMap!==void 0&&(this.emissiveMap=e[t.emissiveMap]||null),t.emissiveIntensity!==void 0&&(this.emissiveIntensity=t.emissiveIntensity),t.specularMap!==void 0&&(this.specularMap=e[t.specularMap]||null),t.specularIntensityMap!==void 0&&(this.specularIntensityMap=e[t.specularIntensityMap]||null),t.specularColorMap!==void 0&&(this.specularColorMap=e[t.specularColorMap]||null),t.envMap!==void 0&&(this.envMap=e[t.envMap]||null),t.envMapRotation!==void 0&&this.envMapRotation.fromArray(t.envMapRotation),t.envMapIntensity!==void 0&&(this.envMapIntensity=t.envMapIntensity),t.reflectivity!==void 0&&(this.reflectivity=t.reflectivity),t.refractionRatio!==void 0&&(this.refractionRatio=t.refractionRatio),t.lightMap!==void 0&&(this.lightMap=e[t.lightMap]||null),t.lightMapIntensity!==void 0&&(this.lightMapIntensity=t.lightMapIntensity),t.aoMap!==void 0&&(this.aoMap=e[t.aoMap]||null),t.aoMapIntensity!==void 0&&(this.aoMapIntensity=t.aoMapIntensity),t.gradientMap!==void 0&&(this.gradientMap=e[t.gradientMap]||null),t.clearcoatMap!==void 0&&(this.clearcoatMap=e[t.clearcoatMap]||null),t.clearcoatRoughnessMap!==void 0&&(this.clearcoatRoughnessMap=e[t.clearcoatRoughnessMap]||null),t.clearcoatNormalMap!==void 0&&(this.clearcoatNormalMap=e[t.clearcoatNormalMap]||null),t.clearcoatNormalScale!==void 0&&(this.clearcoatNormalScale=new Ct().fromArray(t.clearcoatNormalScale)),t.iridescenceMap!==void 0&&(this.iridescenceMap=e[t.iridescenceMap]||null),t.iridescenceThicknessMap!==void 0&&(this.iridescenceThicknessMap=e[t.iridescenceThicknessMap]||null),t.transmissionMap!==void 0&&(this.transmissionMap=e[t.transmissionMap]||null),t.thicknessMap!==void 0&&(this.thicknessMap=e[t.thicknessMap]||null),t.anisotropyMap!==void 0&&(this.anisotropyMap=e[t.anisotropyMap]||null),t.sheenColorMap!==void 0&&(this.sheenColorMap=e[t.sheenColorMap]||null),t.sheenRoughnessMap!==void 0&&(this.sheenRoughnessMap=e[t.sheenRoughnessMap]||null),this}clone(){return new this.constructor().copy(this)}copy(t){this.name=t.name,this.blending=t.blending,this.side=t.side,this.vertexColors=t.vertexColors,this.opacity=t.opacity,this.transparent=t.transparent,this.blendSrc=t.blendSrc,this.blendDst=t.blendDst,this.blendEquation=t.blendEquation,this.blendSrcAlpha=t.blendSrcAlpha,this.blendDstAlpha=t.blendDstAlpha,this.blendEquationAlpha=t.blendEquationAlpha,this.blendColor.copy(t.blendColor),this.blendAlpha=t.blendAlpha,this.depthFunc=t.depthFunc,this.depthTest=t.depthTest,this.depthWrite=t.depthWrite,this.stencilWriteMask=t.stencilWriteMask,this.stencilFunc=t.stencilFunc,this.stencilRef=t.stencilRef,this.stencilFuncMask=t.stencilFuncMask,this.stencilFail=t.stencilFail,this.stencilZFail=t.stencilZFail,this.stencilZPass=t.stencilZPass,this.stencilWrite=t.stencilWrite;const e=t.clippingPlanes;let n=null;if(e!==null){const r=e.length;n=new Array(r);for(let s=0;s!==r;++s)n[s]=e[s].clone()}return this.clippingPlanes=n,this.clipIntersection=t.clipIntersection,this.clipShadows=t.clipShadows,this.shadowSide=t.shadowSide,this.colorWrite=t.colorWrite,this.precision=t.precision,this.polygonOffset=t.polygonOffset,this.polygonOffsetFactor=t.polygonOffsetFactor,this.polygonOffsetUnits=t.polygonOffsetUnits,this.dithering=t.dithering,this.alphaTest=t.alphaTest,this.alphaHash=t.alphaHash,this.alphaToCoverage=t.alphaToCoverage,this.premultipliedAlpha=t.premultipliedAlpha,this.forceSinglePass=t.forceSinglePass,this.allowOverride=t.allowOverride,this.visible=t.visible,this.toneMapped=t.toneMapped,this.userData=JSON.parse(JSON.stringify(t.userData)),this}dispose(){this.dispatchEvent({type:"dispose"})}set needsUpdate(t){t===!0&&this.version++}}const mn=new I,ds=new I,tr=new I,er=new I;class Br{constructor(t=new I,e=new I(0,0,-1)){this.origin=t,this.direction=e}set(t,e){return this.origin.copy(t),this.direction.copy(e),this}copy(t){return this.origin.copy(t.origin),this.direction.copy(t.direction),this}at(t,e){return e.copy(this.origin).addScaledVector(this.direction,t)}lookAt(t){return this.direction.copy(t).sub(this.origin).normalize(),this}recast(t){return this.origin.copy(this.at(t,mn)),this}closestPointToPoint(t,e){e.subVectors(t,this.origin);const n=e.dot(this.direction);return n<0?e.copy(this.origin):e.copy(this.origin).addScaledVector(this.direction,n)}distanceToPoint(t){return Math.sqrt(this.distanceSqToPoint(t))}distanceSqToPoint(t){const e=mn.subVectors(t,this.origin).dot(this.direction);return e<0?this.origin.distanceToSquared(t):(mn.copy(this.origin).addScaledVector(this.direction,e),mn.distanceToSquared(t))}distanceSqToSegment(t,e,n,r){ds.copy(t).add(e).multiplyScalar(.5),tr.copy(e).sub(t).normalize(),er.copy(this.origin).sub(ds);const s=t.distanceTo(e)*.5,a=-this.direction.dot(tr),o=er.dot(this.direction),c=-er.dot(tr),l=er.lengthSq(),d=Math.abs(1-a*a);let p,h,_,v;if(d>0)if(p=a*c-o,h=a*o-c,v=s*d,p>=0)if(h>=-v)if(h<=v){const M=1/d;p*=M,h*=M,_=p*(p+a*h+2*o)+h*(a*p+h+2*c)+l}else h=s,p=Math.max(0,-(a*h+o)),_=-p*p+h*(h+2*c)+l;else h=-s,p=Math.max(0,-(a*h+o)),_=-p*p+h*(h+2*c)+l;else h<=-v?(p=Math.max(0,-(-a*s+o)),h=p>0?-s:Math.min(Math.max(-s,-c),s),_=-p*p+h*(h+2*c)+l):h<=v?(p=0,h=Math.min(Math.max(-s,-c),s),_=h*(h+2*c)+l):(p=Math.max(0,-(a*s+o)),h=p>0?s:Math.min(Math.max(-s,-c),s),_=-p*p+h*(h+2*c)+l);else h=a>0?-s:s,p=Math.max(0,-(a*h+o)),_=-p*p+h*(h+2*c)+l;return n&&n.copy(this.origin).addScaledVector(this.direction,p),r&&r.copy(ds).addScaledVector(tr,h),_}intersectSphere(t,e){if(t.radius<0)return null;mn.subVectors(t.center,this.origin);const n=mn.dot(this.direction),r=mn.dot(mn)-n*n,s=t.radius*t.radius;if(r>s)return null;const a=Math.sqrt(s-r),o=n-a,c=n+a;return c<0?null:o<0?this.at(c,e):this.at(o,e)}intersectsSphere(t){return t.radius<0?!1:this.distanceSqToPoint(t.center)<=t.radius*t.radius}distanceToPlane(t){const e=t.normal.dot(this.direction);if(e===0)return t.distanceToPoint(this.origin)===0?0:null;const n=-(this.origin.dot(t.normal)+t.constant)/e;return n>=0?n:null}intersectPlane(t,e){const n=this.distanceToPlane(t);return n===null?null:this.at(n,e)}intersectsPlane(t){const e=t.distanceToPoint(this.origin);return e===0||t.normal.dot(this.direction)*e<0}intersectBox(t,e){let n,r,s,a,o,c;const l=1/this.direction.x,d=1/this.direction.y,p=1/this.direction.z,h=this.origin;return l>=0?(n=(t.min.x-h.x)*l,r=(t.max.x-h.x)*l):(n=(t.max.x-h.x)*l,r=(t.min.x-h.x)*l),d>=0?(s=(t.min.y-h.y)*d,a=(t.max.y-h.y)*d):(s=(t.max.y-h.y)*d,a=(t.min.y-h.y)*d),n>a||s>r||((s>n||isNaN(n))&&(n=s),(a<r||isNaN(r))&&(r=a),p>=0?(o=(t.min.z-h.z)*p,c=(t.max.z-h.z)*p):(o=(t.max.z-h.z)*p,c=(t.min.z-h.z)*p),n>c||o>r)||((o>n||n!==n)&&(n=o),(c<r||r!==r)&&(r=c),r<0)?null:this.at(n>=0?n:r,e)}intersectsBox(t){return this.intersectBox(t,mn)!==null}intersectTriangle(t,e,n,r,s){const a=this.origin,o=this.direction,c=o.x,l=o.y,d=o.z,p=t.x-a.x,h=t.y-a.y,_=t.z-a.z,v=e.x-a.x,M=e.y-a.y,m=e.z-a.z,u=n.x-a.x,A=n.y-a.y,P=n.z-a.z,S=Math.abs(c),b=Math.abs(l),y=Math.abs(d);let w,x,T,L,F,G,k,U,H,K,Z,nt;if(S>=b&&S>=y?(T=c,G=p,H=v,nt=u,c>=0?(w=l,x=d,L=h,F=_,k=M,U=m,K=A,Z=P):(w=d,x=l,L=_,F=h,k=m,U=M,K=P,Z=A)):b>=y?(T=l,G=h,H=M,nt=A,l>=0?(w=d,x=c,L=_,F=p,k=m,U=v,K=P,Z=u):(w=c,x=d,L=p,F=_,k=v,U=m,K=u,Z=P)):(T=d,G=_,H=m,nt=P,d>=0?(w=c,x=l,L=p,F=h,k=v,U=M,K=u,Z=A):(w=l,x=c,L=h,F=p,k=M,U=v,K=A,Z=u)),T===0)return null;const X=w/T,j=x/T,et=1/T,At=L-X*G,bt=F-j*G,ne=k-X*H,Vt=U-j*H,Yt=K-X*nt,Y=Z-j*nt,Q=Yt*Vt-Y*ne,gt=At*Y-bt*Yt,Pt=ne*bt-Vt*At;if(r){if(Q<0||gt<0||Pt<0)return null}else if((Q<0||gt<0||Pt<0)&&(Q>0||gt>0||Pt>0))return null;const mt=Q+gt+Pt;if(mt===0)return null;const Ft=et*(Q*G+gt*H+Pt*nt);return(mt>0?Ft<0:Ft>0)?null:this.at(Ft/mt,s)}applyMatrix4(t){return this.origin.applyMatrix4(t),this.direction.transformDirection(t),this}equals(t){return t.origin.equals(this.origin)&&t.direction.equals(this.direction)}clone(){return new this.constructor().copy(this)}}class wl extends Mi{constructor(t){super(),this.isMeshBasicMaterial=!0,this.type="MeshBasicMaterial",this.color=new Gt(16777215),this.map=null,this.lightMap=null,this.lightMapIntensity=1,this.aoMap=null,this.aoMapIntensity=1,this.specularMap=null,this.alphaMap=null,this.envMap=null,this.envMapRotation=new In,this.combine=rl,this.reflectivity=1,this.refractionRatio=.98,this.wireframe=!1,this.wireframeLinewidth=1,this.wireframeLinecap="round",this.wireframeLinejoin="round",this.fog=!0,this.setValues(t)}copy(t){return super.copy(t),this.color.copy(t.color),this.map=t.map,this.lightMap=t.lightMap,this.lightMapIntensity=t.lightMapIntensity,this.aoMap=t.aoMap,this.aoMapIntensity=t.aoMapIntensity,this.specularMap=t.specularMap,this.alphaMap=t.alphaMap,this.envMap=t.envMap,this.envMapRotation.copy(t.envMapRotation),this.combine=t.combine,this.reflectivity=t.reflectivity,this.refractionRatio=t.refractionRatio,this.wireframe=t.wireframe,this.wireframeLinewidth=t.wireframeLinewidth,this.wireframeLinecap=t.wireframeLinecap,this.wireframeLinejoin=t.wireframeLinejoin,this.fog=t.fog,this}}const co=new ee,Bn=new Br,nr=new Si,ho=new I,ir=new I,rr=new I,sr=new I,fs=new I,ar=new I,uo=new I,or=new I;class je extends Se{constructor(t=new Ne,e=new wl){super(),this.isMesh=!0,this.type="Mesh",this.geometry=t,this.material=e,this.morphTargetDictionary=void 0,this.morphTargetInfluences=void 0,this.count=1,this.updateMorphTargets()}copy(t,e){return super.copy(t,e),t.morphTargetInfluences!==void 0&&(this.morphTargetInfluences=t.morphTargetInfluences.slice()),t.morphTargetDictionary!==void 0&&(this.morphTargetDictionary=Object.assign({},t.morphTargetDictionary)),this.material=Array.isArray(t.material)?t.material.slice():t.material,this.geometry=t.geometry,this}updateMorphTargets(){const e=this.geometry.morphAttributes,n=Object.keys(e);if(n.length>0){const r=e[n[0]];if(r!==void 0){this.morphTargetInfluences=[],this.morphTargetDictionary={};for(let s=0,a=r.length;s<a;s++){const o=r[s].name||String(s);this.morphTargetInfluences.push(0),this.morphTargetDictionary[o]=s}}}}getVertexPosition(t,e){const n=this.geometry,r=n.attributes.position,s=n.morphAttributes.position,a=n.morphTargetsRelative;e.fromBufferAttribute(r,t);const o=this.morphTargetInfluences;if(s&&o){ar.set(0,0,0);for(let c=0,l=s.length;c<l;c++){const d=o[c],p=s[c];d!==0&&(fs.fromBufferAttribute(p,t),a?ar.addScaledVector(fs,d):ar.addScaledVector(fs.sub(e),d))}e.add(ar)}return e}intersectsFrustum(t){return t.intersectsObject(this)}raycast(t,e){const n=this.geometry,r=this.material,s=this.matrixWorld;r!==void 0&&(n.boundingSphere===null&&n.computeBoundingSphere(),nr.copy(n.boundingSphere),nr.applyMatrix4(s),Bn.copy(t.ray).recast(t.near),!(nr.containsPoint(Bn.origin)===!1&&(Bn.intersectSphere(nr,ho)===null||Bn.origin.distanceToSquared(ho)>(t.far-t.near)**2))&&(co.copy(s).invert(),Bn.copy(t.ray).applyMatrix4(co),!(n.boundingBox!==null&&Bn.intersectsBox(n.boundingBox)===!1)&&this._computeIntersections(t,e,Bn)))}_computeIntersections(t,e,n){let r;const s=this.geometry,a=this.material,o=s.index,c=s.attributes.position,l=s.attributes.uv,d=s.attributes.uv1,p=s.attributes.normal,h=s.groups,_=s.drawRange;if(o!==null)if(Array.isArray(a))for(let v=0,M=h.length;v<M;v++){const m=h[v],u=a[m.materialIndex],A=Math.max(m.start,_.start),P=Math.min(o.count,Math.min(m.start+m.count,_.start+_.count));for(let S=A,b=P;S<b;S+=3){const y=o.getX(S),w=o.getX(S+1),x=o.getX(S+2);r=lr(this,u,t,n,l,d,p,y,w,x),r&&(r.faceIndex=Math.floor(S/3),r.face.materialIndex=m.materialIndex,e.push(r))}}else{const v=Math.max(0,_.start),M=Math.min(o.count,_.start+_.count);for(let m=v,u=M;m<u;m+=3){const A=o.getX(m),P=o.getX(m+1),S=o.getX(m+2);r=lr(this,a,t,n,l,d,p,A,P,S),r&&(r.faceIndex=Math.floor(m/3),e.push(r))}}else if(c!==void 0)if(Array.isArray(a))for(let v=0,M=h.length;v<M;v++){const m=h[v],u=a[m.materialIndex],A=Math.max(m.start,_.start),P=Math.min(c.count,Math.min(m.start+m.count,_.start+_.count));for(let S=A,b=P;S<b;S+=3){const y=S,w=S+1,x=S+2;r=lr(this,u,t,n,l,d,p,y,w,x),r&&(r.faceIndex=Math.floor(S/3),r.face.materialIndex=m.materialIndex,e.push(r))}}else{const v=Math.max(0,_.start),M=Math.min(c.count,_.start+_.count);for(let m=v,u=M;m<u;m+=3){const A=m,P=m+1,S=m+2;r=lr(this,a,t,n,l,d,p,A,P,S),r&&(r.faceIndex=Math.floor(m/3),e.push(r))}}}}function _h(i,t,e,n,r,s,a,o){let c;if(t.side===Ue?c=n.intersectTriangle(a,s,r,!0,o):c=n.intersectTriangle(r,s,a,t.side===kn,o),c===null)return null;or.copy(o),or.applyMatrix4(i.matrixWorld);const l=e.ray.origin.distanceTo(or);return l<e.near||l>e.far?null:{distance:l,point:or.clone(),object:i}}function lr(i,t,e,n,r,s,a,o,c,l){i.getVertexPosition(o,ir),i.getVertexPosition(c,rr),i.getVertexPosition(l,sr);const d=_h(i,t,e,n,ir,rr,sr,uo);if(d){const p=new I;Ve.getBarycoord(uo,ir,rr,sr,p),r&&(d.uv=Ve.getInterpolatedAttribute(r,o,c,l,p,new Ct)),s&&(d.uv1=Ve.getInterpolatedAttribute(s,o,c,l,p,new Ct)),a&&(d.normal=Ve.getInterpolatedAttribute(a,o,c,l,p,new I),d.normal.dot(n.direction)>0&&d.normal.multiplyScalar(-1));const h={a:o,b:c,c:l,normal:new I,materialIndex:0};Ve.getNormal(ir,rr,sr,h.normal),d.face=h,d.barycoord=p}return d}class Rl extends De{constructor(t=null,e=1,n=1,r,s,a,o,c,l=Me,d=Me,p,h){super(null,a,o,c,l,d,r,s,p,h),this.isDataTexture=!0,this.image={data:t,width:e,height:n},this.generateMipmaps=!1,this.flipY=!1,this.unpackAlignment=1}}class fo extends Qe{constructor(t,e,n,r=1){super(t,e,n),this.isInstancedBufferAttribute=!0,this.meshPerAttribute=r}copy(t){return super.copy(t),this.meshPerAttribute=t.meshPerAttribute,this}toJSON(){const t=super.toJSON();return t.meshPerAttribute=this.meshPerAttribute,t.isInstancedBufferAttribute=!0,t}}const si=new ee,po=new ee,cr=[],mo=new Yn,gh=new ee,wi=new je,Ri=new Si;class Km extends je{constructor(t,e,n){super(t,e),this.isInstancedMesh=!0,this.instanceMatrix=new fo(new Float32Array(n*16),16),this.instanceColor=null,this.morphTexture=null,this.count=n,this.boundingBox=null,this.boundingSphere=null;for(let r=0;r<n;r++)this.setMatrixAt(r,gh)}computeBoundingBox(){const t=this.geometry,e=this.count;this.boundingBox===null&&(this.boundingBox=new Yn),t.boundingBox===null&&t.computeBoundingBox(),this.boundingBox.makeEmpty();for(let n=0;n<e;n++)this.getMatrixAt(n,si),mo.copy(t.boundingBox).applyMatrix4(si),this.boundingBox.union(mo)}computeBoundingSphere(){const t=this.geometry,e=this.count;this.boundingSphere===null&&(this.boundingSphere=new Si),t.boundingSphere===null&&t.computeBoundingSphere(),this.boundingSphere.makeEmpty();for(let n=0;n<e;n++)this.getMatrixAt(n,si),Ri.copy(t.boundingSphere).applyMatrix4(si),this.boundingSphere.union(Ri)}copy(t,e){return super.copy(t,e),this.instanceMatrix.copy(t.instanceMatrix),t.morphTexture!==null&&(this.morphTexture=t.morphTexture.clone()),t.instanceColor!==null&&(this.instanceColor=t.instanceColor.clone()),this.count=t.count,t.boundingBox!==null&&(this.boundingBox=t.boundingBox.clone()),t.boundingSphere!==null&&(this.boundingSphere=t.boundingSphere.clone()),this}getColorAt(t,e){return this.instanceColor===null?e.setRGB(1,1,1):e.fromArray(this.instanceColor.array,t*3)}getMatrixAt(t,e){return e.fromArray(this.instanceMatrix.array,t*16)}getMorphAt(t,e){const n=e.morphTargetInfluences,r=this.morphTexture.source.data.data,s=n.length+1,a=t*s+1;for(let o=0;o<n.length;o++)n[o]=r[a+o]}raycast(t,e){const n=this.matrixWorld,r=this.count;if(wi.geometry=this.geometry,wi.material=this.material,wi.material!==void 0&&(this.boundingSphere===null&&this.computeBoundingSphere(),Ri.copy(this.boundingSphere),Ri.applyMatrix4(n),t.ray.intersectsSphere(Ri)!==!1))for(let s=0;s<r;s++){this.getMatrixAt(s,si),po.multiplyMatrices(n,si),wi.matrixWorld=po,wi.raycast(t,cr);for(let a=0,o=cr.length;a<o;a++){const c=cr[a];c.instanceId=s,c.object=this,e.push(c)}cr.length=0}}setColorAt(t,e){return this.instanceColor===null&&(this.instanceColor=new fo(new Float32Array(this.instanceMatrix.count*3).fill(1),3)),e.toArray(this.instanceColor.array,t*3),this}setMatrixAt(t,e){return e.toArray(this.instanceMatrix.array,t*16),this}setMorphAt(t,e){const n=e.morphTargetInfluences,r=n.length+1;this.morphTexture===null&&(this.morphTexture=new Rl(new Float32Array(r*this.count),r,this.count,ga,Ke));const s=this.morphTexture.source.data.data;let a=0;for(let l=0;l<n.length;l++)a+=n[l];const o=this.geometry.morphTargetsRelative?1:1-a,c=r*t;return s[c]=o,s.set(n,c+1),this}updateMorphTargets(){}dispose(){super.dispose(),this.morphTexture!==null&&(this.morphTexture.dispose(),this.morphTexture=null)}}const zn=new Si,xh=new Ct(.5,.5),hr=new I;class Aa{constructor(t=new _n,e=new _n,n=new _n,r=new _n,s=new _n,a=new _n){this.planes=[t,e,n,r,s,a]}set(t,e,n,r,s,a){const o=this.planes;return o[0].copy(t),o[1].copy(e),o[2].copy(n),o[3].copy(r),o[4].copy(s),o[5].copy(a),this}copy(t){const e=this.planes;for(let n=0;n<6;n++)e[n].copy(t.planes[n]);return this}setFromProjectionMatrix(t,e=an,n=!1){const r=this.planes,s=t.elements,a=s[0],o=s[1],c=s[2],l=s[3],d=s[4],p=s[5],h=s[6],_=s[7],v=s[8],M=s[9],m=s[10],u=s[11],A=s[12],P=s[13],S=s[14],b=s[15];if(r[0].setComponents(l-a,_-d,u-v,b-A).normalize(),r[1].setComponents(l+a,_+d,u+v,b+A).normalize(),r[2].setComponents(l+o,_+p,u+M,b+P).normalize(),r[3].setComponents(l-o,_-p,u-M,b-P).normalize(),n)r[4].setComponents(c,h,m,S).normalize(),r[5].setComponents(l-c,_-h,u-m,b-S).normalize();else if(r[4].setComponents(l-c,_-h,u-m,b-S).normalize(),e===an)r[5].setComponents(l+c,_+h,u+m,b+S).normalize();else if(e===zi)r[5].setComponents(c,h,m,S).normalize();else throw new Error("THREE.Frustum.setFromProjectionMatrix(): Invalid coordinate system: "+e);return this}intersectsObject(t){if(t.boundingSphere!==void 0)t.boundingSphere===null&&t.computeBoundingSphere(),zn.copy(t.boundingSphere).applyMatrix4(t.matrixWorld);else{const e=t.geometry;e.boundingSphere===null&&e.computeBoundingSphere(),zn.copy(e.boundingSphere).applyMatrix4(t.matrixWorld)}return this.intersectsSphere(zn)}intersectsSprite(t){zn.center.set(0,0,0);const e=xh.distanceTo(t.center);return zn.radius=.7071067811865476+e,zn.applyMatrix4(t.matrixWorld),this.intersectsSphere(zn)}intersectsSphere(t){const e=this.planes,n=t.center,r=-t.radius;for(let s=0;s<6;s++)if(e[s].distanceToPoint(n)<r)return!1;return!0}intersectsBox(t){const e=this.planes;for(let n=0;n<6;n++){const r=e[n];if(hr.x=r.normal.x>0?t.max.x:t.min.x,hr.y=r.normal.y>0?t.max.y:t.min.y,hr.z=r.normal.z>0?t.max.z:t.min.z,r.distanceToPoint(hr)<0)return!1}return!0}containsPoint(t){const e=this.planes;for(let n=0;n<6;n++)if(e[n].distanceToPoint(t)<0)return!1;return!0}clone(){return new this.constructor().copy(this)}}class vh extends Mi{constructor(t){super(),this.isLineBasicMaterial=!0,this.type="LineBasicMaterial",this.color=new Gt(16777215),this.map=null,this.linewidth=1,this.linecap="round",this.linejoin="round",this.fog=!0,this.setValues(t)}copy(t){return super.copy(t),this.color.copy(t.color),this.map=t.map,this.linewidth=t.linewidth,this.linecap=t.linecap,this.linejoin=t.linejoin,this.fog=t.fog,this}}const Ur=new I,Nr=new I,_o=new ee,Ci=new Br,ur=new Si,ps=new I,go=new I;class Sh extends Se{constructor(t=new Ne,e=new vh){super(),this.isLine=!0,this.type="Line",this.geometry=t,this.material=e,this.morphTargetDictionary=void 0,this.morphTargetInfluences=void 0,this.updateMorphTargets()}copy(t,e){return super.copy(t,e),this.material=Array.isArray(t.material)?t.material.slice():t.material,this.geometry=t.geometry,this}computeLineDistances(){const t=this.geometry;if(t.index===null){const e=t.attributes.position,n=[0];for(let r=1,s=e.count;r<s;r++)Ur.fromBufferAttribute(e,r-1),Nr.fromBufferAttribute(e,r),n[r]=n[r-1],n[r]+=Ur.distanceTo(Nr);t.setAttribute("lineDistance",new we(n,1))}else Rt("Line.computeLineDistances(): Computation only possible with non-indexed BufferGeometry.");return this}intersectsFrustum(t){return t.intersectsObject(this)}raycast(t,e){const n=this.geometry,r=this.matrixWorld,s=t.params.Line.threshold,a=n.drawRange;if(n.boundingSphere===null&&n.computeBoundingSphere(),ur.copy(n.boundingSphere),ur.applyMatrix4(r),ur.radius+=s,t.ray.intersectsSphere(ur)===!1)return;_o.copy(r).invert(),Ci.copy(t.ray).applyMatrix4(_o);const o=s/((this.scale.x+this.scale.y+this.scale.z)/3),c=o*o,l=this.isLineSegments?2:1,d=n.index,h=n.attributes.position;if(d!==null){const _=Math.max(0,a.start),v=Math.min(d.count,a.start+a.count);for(let M=_,m=v-1;M<m;M+=l){const u=d.getX(M),A=d.getX(M+1),P=dr(this,t,Ci,c,u,A,M);P&&e.push(P)}if(this.isLineLoop){const M=d.getX(v-1),m=d.getX(_),u=dr(this,t,Ci,c,M,m,v-1);u&&e.push(u)}}else{const _=Math.max(0,a.start),v=Math.min(h.count,a.start+a.count);for(let M=_,m=v-1;M<m;M+=l){const u=dr(this,t,Ci,c,M,M+1,M);u&&e.push(u)}if(this.isLineLoop){const M=dr(this,t,Ci,c,v-1,_,v-1);M&&e.push(M)}}}updateMorphTargets(){const e=this.geometry.morphAttributes,n=Object.keys(e);if(n.length>0){const r=e[n[0]];if(r!==void 0){this.morphTargetInfluences=[],this.morphTargetDictionary={};for(let s=0,a=r.length;s<a;s++){const o=r[s].name||String(s);this.morphTargetInfluences.push(0),this.morphTargetDictionary[o]=s}}}}}function dr(i,t,e,n,r,s,a){const o=i.geometry.attributes.position;if(Ur.fromBufferAttribute(o,r),Nr.fromBufferAttribute(o,s),e.distanceSqToSegment(Ur,Nr,ps,go)>n)return;ps.applyMatrix4(i.matrixWorld);const l=t.ray.origin.distanceTo(ps);if(!(l<t.near||l>t.far))return{distance:l,point:go.clone().applyMatrix4(i.matrixWorld),index:a,face:null,faceIndex:null,barycoord:null,object:i}}const xo=new I,vo=new I;class $m extends Sh{constructor(t,e){super(t,e),this.isLineSegments=!0,this.type="LineSegments"}computeLineDistances(){const t=this.geometry;if(t.index===null){const e=t.attributes.position,n=[];for(let r=0,s=e.count;r<s;r+=2)xo.fromBufferAttribute(e,r),vo.fromBufferAttribute(e,r+1),n[r]=r===0?0:n[r-1],n[r+1]=n[r]+xo.distanceTo(vo);t.setAttribute("lineDistance",new we(n,1))}else Rt("LineSegments.computeLineDistances(): Computation only possible with non-indexed BufferGeometry.");return this}}class Cl extends De{constructor(t=[],e=Wn,n,r,s,a,o,c,l,d){super(t,e,n,r,s,a,o,c,l,d),this.isCubeTexture=!0,this.flipY=!1}get images(){return this.image}set images(t){this.image=t}}class Hi extends De{constructor(t,e,n=ln,r,s,a,o=Me,c=Me,l,d=En,p=1){if(d!==En&&d!==Vn)throw new Error("THREE.DepthTexture: format must be either THREE.DepthFormat or THREE.DepthStencilFormat");const h={width:t,height:e,depth:p};super(h,r,s,a,o,c,d,n,l),this.isDepthTexture=!0,this.flipY=!1,this.generateMipmaps=!1,this.compareFunction=null}copy(t){return super.copy(t),this.source=new ba(Object.assign({},t.image)),this.compareFunction=t.compareFunction,this}toJSON(t){const e=super.toJSON(t);return e.compareFunction=this.compareFunction,e}}class Mh extends Hi{constructor(t,e=ln,n=Wn,r,s,a=Me,o=Me,c,l=En){const d={width:t,height:t,depth:1},p=[d,d,d,d,d,d];super(t,t,e,n,r,s,a,o,c,l),this.image=p,this.isCubeDepthTexture=!0,this.isCubeTexture=!0}get images(){return this.image}set images(t){this.image=t}}class Pl extends De{constructor(t=null){super(),this.sourceTexture=t,this.isExternalTexture=!0}copy(t){return super.copy(t),this.sourceTexture=t.sourceTexture,this}}class Vi extends Ne{constructor(t=1,e=1,n=1,r=1,s=1,a=1){super(),this.type="BoxGeometry",this.parameters={width:t,height:e,depth:n,widthSegments:r,heightSegments:s,depthSegments:a};const o=this;r=Math.floor(r),s=Math.floor(s),a=Math.floor(a);const c=[],l=[],d=[],p=[];let h=0,_=0;v("z","y","x",-1,-1,n,e,t,a,s,0),v("z","y","x",1,-1,n,e,-t,a,s,1),v("x","z","y",1,1,t,n,e,r,a,2),v("x","z","y",1,-1,t,n,-e,r,a,3),v("x","y","z",1,-1,t,e,n,r,s,4),v("x","y","z",-1,-1,t,e,-n,r,s,5),this.setIndex(c),this.setAttribute("position",new we(l,3)),this.setAttribute("normal",new we(d,3)),this.setAttribute("uv",new we(p,2));function v(M,m,u,A,P,S,b,y,w,x,T){const L=S/w,F=b/x,G=S/2,k=b/2,U=y/2,H=w+1,K=x+1;let Z=0,nt=0;const X=new I;for(let j=0;j<K;j++){const et=j*F-k;for(let At=0;At<H;At++){const bt=At*L-G;X[M]=bt*A,X[m]=et*P,X[u]=U,l.push(X.x,X.y,X.z),X[M]=0,X[m]=0,X[u]=y>0?1:-1,d.push(X.x,X.y,X.z),p.push(At/w),p.push(1-j/x),Z+=1}}for(let j=0;j<x;j++)for(let et=0;et<w;et++){const At=h+et+H*j,bt=h+et+H*(j+1),ne=h+(et+1)+H*(j+1),Vt=h+(et+1)+H*j;c.push(At,bt,Vt),c.push(bt,ne,Vt),nt+=6}o.addGroup(_,nt,T),_+=nt,h+=Z}}copy(t){return super.copy(t),this.parameters=Object.assign({},t.parameters),this}static fromJSON(t){return new Vi(t.width,t.height,t.depth,t.widthSegments,t.heightSegments,t.depthSegments)}}const fr=new I,pr=new I,ms=new I,mr=new Ve;class Jm extends Ne{constructor(t=null,e=1){if(super(),this.type="EdgesGeometry",this.parameters={geometry:t,thresholdAngle:e},t!==null){const r=Math.pow(10,4),s=Math.cos(_i*e),a=t.getIndex(),o=t.getAttribute("position"),c=a?a.count:o.count,l=[0,0,0],d=["a","b","c"],p=new Array(3),h={},_=[];for(let v=0;v<c;v+=3){a?(l[0]=a.getX(v),l[1]=a.getX(v+1),l[2]=a.getX(v+2)):(l[0]=v,l[1]=v+1,l[2]=v+2);const{a:M,b:m,c:u}=mr;if(M.fromBufferAttribute(o,l[0]),m.fromBufferAttribute(o,l[1]),u.fromBufferAttribute(o,l[2]),mr.getNormal(ms),p[0]=`${Math.round(M.x*r)},${Math.round(M.y*r)},${Math.round(M.z*r)}`,p[1]=`${Math.round(m.x*r)},${Math.round(m.y*r)},${Math.round(m.z*r)}`,p[2]=`${Math.round(u.x*r)},${Math.round(u.y*r)},${Math.round(u.z*r)}`,!(p[0]===p[1]||p[1]===p[2]||p[2]===p[0]))for(let A=0;A<3;A++){const P=(A+1)%3,S=p[A],b=p[P],y=mr[d[A]],w=mr[d[P]],x=`${S}_${b}`,T=`${b}_${S}`;T in h&&h[T]?(ms.dot(h[T].normal)<=s&&(_.push(y.x,y.y,y.z),_.push(w.x,w.y,w.z)),h[T]=null):x in h||(h[x]={index0:l[A],index1:l[P],normal:ms.clone()})}}for(const v in h)if(h[v]){const{index0:M,index1:m}=h[v];fr.fromBufferAttribute(o,M),pr.fromBufferAttribute(o,m),_.push(fr.x,fr.y,fr.z),_.push(pr.x,pr.y,pr.z)}this.setAttribute("position",new we(_,3))}}copy(t){return super.copy(t),this.parameters=Object.assign({},t.parameters),this}}class zr extends Ne{constructor(t=1,e=1,n=1,r=1){super(),this.type="PlaneGeometry",this.parameters={width:t,height:e,widthSegments:n,heightSegments:r};const s=t/2,a=e/2,o=Math.floor(n),c=Math.floor(r),l=o+1,d=c+1,p=t/o,h=e/c,_=[],v=[],M=[],m=[];for(let u=0;u<d;u++){const A=u*h-a;for(let P=0;P<l;P++){const S=P*p-s;v.push(S,-A,0),M.push(0,0,1),m.push(P/o),m.push(1-u/c)}}for(let u=0;u<c;u++)for(let A=0;A<o;A++){const P=A+l*u,S=A+l*(u+1),b=A+1+l*(u+1),y=A+1+l*u;_.push(P,S,y),_.push(S,b,y)}this.setIndex(_),this.setAttribute("position",new we(v,3)),this.setAttribute("normal",new we(M,3)),this.setAttribute("uv",new we(m,2))}copy(t){return super.copy(t),this.parameters=Object.assign({},t.parameters),this}static fromJSON(t){return new zr(t.width,t.height,t.widthSegments,t.heightSegments)}}class Dl extends Ne{constructor(t=1,e=32,n=16,r=0,s=Math.PI*2,a=0,o=Math.PI){super(),this.type="SphereGeometry",this.parameters={radius:t,widthSegments:e,heightSegments:n,phiStart:r,phiLength:s,thetaStart:a,thetaLength:o},e=Math.max(3,Math.floor(e)),n=Math.max(2,Math.floor(n));const c=Math.min(a+o,Math.PI);let l=0;const d=[],p=new I,h=new I,_=[],v=[],M=[],m=[];for(let u=0;u<=n;u++){const A=[],P=u/n,S=a+P*o,b=t*Math.cos(S),y=Math.sqrt(t*t-b*b);let w=0;u===0&&a===0?w=.5/e:u===n&&c===Math.PI&&(w=-.5/e);for(let x=0;x<=e;x++){const T=x/e,L=r+T*s;p.x=-y*Math.cos(L),p.y=b,p.z=y*Math.sin(L),v.push(p.x,p.y,p.z),h.copy(p).normalize(),M.push(h.x,h.y,h.z),m.push(T+w,1-P),A.push(l++)}d.push(A)}for(let u=0;u<n;u++)for(let A=0;A<e;A++){const P=d[u][A+1],S=d[u][A],b=d[u+1][A],y=d[u+1][A+1];(u!==0||a>0)&&_.push(P,S,y),(u!==n-1||c<Math.PI)&&_.push(S,b,y)}this.setIndex(_),this.setAttribute("position",new we(v,3)),this.setAttribute("normal",new we(M,3)),this.setAttribute("uv",new we(m,2))}copy(t){return super.copy(t),this.parameters=Object.assign({},t.parameters),this}static fromJSON(t){return new Dl(t.radius,t.widthSegments,t.heightSegments,t.phiStart,t.phiLength,t.thetaStart,t.thetaLength)}}class Qm extends Ne{constructor(t=null){if(super(),this.type="WireframeGeometry",this.parameters={geometry:t},t!==null){const e=[],n=new Set,r=new I,s=new I;if(t.index!==null){const a=t.attributes.position,o=t.index;let c=t.groups;c.length===0&&(c=[{start:0,count:o.count,materialIndex:0}]);for(let l=0,d=c.length;l<d;++l){const p=c[l],h=p.start,_=p.count;for(let v=h,M=h+_;v<M;v+=3)for(let m=0;m<3;m++){const u=o.getX(v+m),A=o.getX(v+(m+1)%3);r.fromBufferAttribute(a,u),s.fromBufferAttribute(a,A),So(r,s,n)===!0&&(e.push(r.x,r.y,r.z),e.push(s.x,s.y,s.z))}}}else{const a=t.attributes.position;for(let o=0,c=a.count/3;o<c;o++)for(let l=0;l<3;l++){const d=3*o+l,p=3*o+(l+1)%3;r.fromBufferAttribute(a,d),s.fromBufferAttribute(a,p),So(r,s,n)===!0&&(e.push(r.x,r.y,r.z),e.push(s.x,s.y,s.z))}}this.setAttribute("position",new we(e,3))}}copy(t){return super.copy(t),this.parameters=Object.assign({},t.parameters),this}}function So(i,t,e){const n=`${i.x},${i.y},${i.z}-${t.x},${t.y},${t.z}`,r=`${t.x},${t.y},${t.z}-${i.x},${i.y},${i.z}`;return e.has(n)===!0||e.has(r)===!0?!1:(e.add(n),e.add(r),!0)}function vi(i){const t={};for(const e in i){t[e]={};for(const n in i[e]){const r=i[e][n];if(Mo(r))r.isRenderTargetTexture?(Rt("UniformsUtils: Textures of render targets cannot be cloned via cloneUniforms() or mergeUniforms()."),t[e][n]=null):t[e][n]=r.clone();else if(Array.isArray(r))if(Mo(r[0])){const s=[];for(let a=0,o=r.length;a<o;a++)s[a]=r[a].clone();t[e][n]=s}else t[e][n]=r.slice();else t[e][n]=r}}return t}function Pe(i){const t={};for(let e=0;e<i.length;e++){const n=vi(i[e]);for(const r in n)t[r]=n[r]}return t}function Mo(i){return i&&(i.isColor||i.isMatrix3||i.isMatrix4||i.isVector2||i.isVector3||i.isVector4||i.isTexture||i.isQuaternion)}function Eh(i){const t=[];for(let e=0;e<i.length;e++)t.push(i[e].clone());return t}function Ll(i){const t=i.getRenderTarget();return t===null?i.outputColorSpace:t.isXRRenderTarget===!0?t.texture.colorSpace:Ht.workingColorSpace}const yh={clone:vi,merge:Pe};var bh=`void main() {
	gl_Position = projectionMatrix * modelViewMatrix * vec4( position, 1.0 );
}`,Th=`void main() {
	gl_FragColor = vec4( 1.0, 0.0, 0.0, 1.0 );
}`;class hn extends Mi{constructor(t){super(),this.isShaderMaterial=!0,this.type="ShaderMaterial",this.defines={},this.uniforms={},this.uniformsGroups=[],this.vertexShader=bh,this.fragmentShader=Th,this.linewidth=1,this.wireframe=!1,this.wireframeLinewidth=1,this.fog=!1,this.lights=!1,this.clipping=!1,this.forceSinglePass=!0,this.extensions={clipCullDistance:!1,multiDraw:!1},this.defaultAttributeValues={color:[1,1,1],uv:[0,0],uv1:[0,0]},this.index0AttributeName=void 0,this.uniformsNeedUpdate=!1,this.glslVersion=null,t!==void 0&&this.setValues(t)}copy(t){return super.copy(t),this.fragmentShader=t.fragmentShader,this.vertexShader=t.vertexShader,this.uniforms=vi(t.uniforms),this.uniformsGroups=Eh(t.uniformsGroups),this.defines=Object.assign({},t.defines),this.wireframe=t.wireframe,this.wireframeLinewidth=t.wireframeLinewidth,this.fog=t.fog,this.lights=t.lights,this.clipping=t.clipping,this.extensions=Object.assign({},t.extensions),this.glslVersion=t.glslVersion,this.defaultAttributeValues=Object.assign({},t.defaultAttributeValues),this.index0AttributeName=t.index0AttributeName,this.uniformsNeedUpdate=t.uniformsNeedUpdate,this}toJSON(t){const e=super.toJSON(t);e.glslVersion=this.glslVersion,e.uniforms={};for(const r in this.uniforms){const a=this.uniforms[r].value;a&&a.isTexture?e.uniforms[r]={type:"t",value:a.toJSON(t).uuid}:a&&a.isColor?e.uniforms[r]={type:"c",value:a.getHex()}:a&&a.isVector2?e.uniforms[r]={type:"v2",value:a.toArray()}:a&&a.isVector3?e.uniforms[r]={type:"v3",value:a.toArray()}:a&&a.isVector4?e.uniforms[r]={type:"v4",value:a.toArray()}:a&&a.isMatrix3?e.uniforms[r]={type:"m3",value:a.toArray()}:a&&a.isMatrix4?e.uniforms[r]={type:"m4",value:a.toArray()}:e.uniforms[r]={value:a}}Object.keys(this.defines).length>0&&(e.defines=this.defines),e.vertexShader=this.vertexShader,e.fragmentShader=this.fragmentShader,e.lights=this.lights,e.clipping=this.clipping;const n={};for(const r in this.extensions)this.extensions[r]===!0&&(n[r]=!0);return Object.keys(n).length>0&&(e.extensions=n),e}fromJSON(t,e){if(super.fromJSON(t,e),t.uniforms!==void 0)for(const n in t.uniforms){const r=t.uniforms[n];switch(this.uniforms[n]={},r.type){case"t":this.uniforms[n].value=e[r.value]||null;break;case"c":this.uniforms[n].value=new Gt().setHex(r.value);break;case"v2":this.uniforms[n].value=new Ct().fromArray(r.value);break;case"v3":this.uniforms[n].value=new I().fromArray(r.value);break;case"v4":this.uniforms[n].value=new ce().fromArray(r.value);break;case"m3":this.uniforms[n].value=new Dt().fromArray(r.value);break;case"m4":this.uniforms[n].value=new ee().fromArray(r.value);break;default:this.uniforms[n].value=r.value}}if(t.defines!==void 0&&(this.defines=t.defines),t.vertexShader!==void 0&&(this.vertexShader=t.vertexShader),t.fragmentShader!==void 0&&(this.fragmentShader=t.fragmentShader),t.glslVersion!==void 0&&(this.glslVersion=t.glslVersion),t.extensions!==void 0)for(const n in t.extensions)this.extensions[n]=t.extensions[n];return t.lights!==void 0&&(this.lights=t.lights),t.clipping!==void 0&&(this.clipping=t.clipping),this}}class Ah extends hn{constructor(t){super(t),this.isRawShaderMaterial=!0,this.type="RawShaderMaterial"}}class jm extends Mi{constructor(t){super(),this.isMeshStandardMaterial=!0,this.type="MeshStandardMaterial",this.defines={STANDARD:""},this.color=new Gt(16777215),this.roughness=1,this.metalness=0,this.map=null,this.lightMap=null,this.lightMapIntensity=1,this.aoMap=null,this.aoMapIntensity=1,this.emissive=new Gt(0),this.emissiveIntensity=1,this.emissiveMap=null,this.bumpMap=null,this.bumpScale=1,this.normalMap=null,this.normalMapType=ha,this.normalScale=new Ct(1,1),this.displacementMap=null,this.displacementScale=1,this.displacementBias=0,this.roughnessMap=null,this.metalnessMap=null,this.alphaMap=null,this.envMap=null,this.envMapRotation=new In,this.envMapIntensity=1,this.wireframe=!1,this.wireframeLinewidth=1,this.wireframeLinecap="round",this.wireframeLinejoin="round",this.flatShading=!1,this.fog=!0,this.setValues(t)}copy(t){return super.copy(t),this.defines={STANDARD:""},this.color.copy(t.color),this.roughness=t.roughness,this.metalness=t.metalness,this.map=t.map,this.lightMap=t.lightMap,this.lightMapIntensity=t.lightMapIntensity,this.aoMap=t.aoMap,this.aoMapIntensity=t.aoMapIntensity,this.emissive.copy(t.emissive),this.emissiveMap=t.emissiveMap,this.emissiveIntensity=t.emissiveIntensity,this.bumpMap=t.bumpMap,this.bumpScale=t.bumpScale,this.normalMap=t.normalMap,this.normalMapType=t.normalMapType,this.normalScale.copy(t.normalScale),this.displacementMap=t.displacementMap,this.displacementScale=t.displacementScale,this.displacementBias=t.displacementBias,this.roughnessMap=t.roughnessMap,this.metalnessMap=t.metalnessMap,this.alphaMap=t.alphaMap,this.envMap=t.envMap,this.envMapRotation.copy(t.envMapRotation),this.envMapIntensity=t.envMapIntensity,this.wireframe=t.wireframe,this.wireframeLinewidth=t.wireframeLinewidth,this.wireframeLinecap=t.wireframeLinecap,this.wireframeLinejoin=t.wireframeLinejoin,this.flatShading=t.flatShading,this.fog=t.fog,this}}class wh extends Mi{constructor(t){super(),this.isMeshDepthMaterial=!0,this.type="MeshDepthMaterial",this.depthPacking=Sc,this.map=null,this.alphaMap=null,this.displacementMap=null,this.displacementScale=1,this.displacementBias=0,this.wireframe=!1,this.wireframeLinewidth=1,this.setValues(t)}copy(t){return super.copy(t),this.depthPacking=t.depthPacking,this.map=t.map,this.alphaMap=t.alphaMap,this.displacementMap=t.displacementMap,this.displacementScale=t.displacementScale,this.displacementBias=t.displacementBias,this.wireframe=t.wireframe,this.wireframeLinewidth=t.wireframeLinewidth,this}}class Rh extends Mi{constructor(t){super(),this.isMeshDistanceMaterial=!0,this.type="MeshDistanceMaterial",this.map=null,this.alphaMap=null,this.displacementMap=null,this.displacementScale=1,this.displacementBias=0,this.setValues(t)}copy(t){return super.copy(t),this.map=t.map,this.alphaMap=t.alphaMap,this.displacementMap=t.displacementMap,this.displacementScale=t.displacementScale,this.displacementBias=t.displacementBias,this}}class wa extends Se{constructor(t,e=1){super(),this.isLight=!0,this.type="Light",this.color=new Gt(t),this.intensity=e}copy(t,e){return super.copy(t,e),this.color.copy(t.color),this.intensity=t.intensity,this}toJSON(t){const e=super.toJSON(t);return e.object.color=this.color.getHex(),e.object.intensity=this.intensity,e}}class t_ extends wa{constructor(t,e,n){super(t,n),this.isHemisphereLight=!0,this.type="HemisphereLight",this.position.copy(Se.DEFAULT_UP),this.updateMatrix(),this.groundColor=new Gt(e)}copy(t,e){return super.copy(t,e),this.groundColor.copy(t.groundColor),this}toJSON(t){const e=super.toJSON(t);return e.object.groundColor=this.groundColor.getHex(),e}}const _s=new ee,Eo=new I,yo=new I;class Ch{constructor(t){this.camera=t,this.intensity=1,this.bias=0,this.biasNode=null,this.normalBias=0,this.radius=1,this.blurSamples=8,this.mapSize=new Ct(512,512),this.mapType=Be,this.map=null,this.mapPass=null,this.matrix=new ee,this.autoUpdate=!0,this.needsUpdate=!1,this._frustum=new Aa,this._frameExtents=new Ct(1,1),this._viewportCount=1,this._viewports=[new ce(0,0,1,1)]}getViewportCount(){return this._viewportCount}getCamera(){return this.camera}getFrustum(){return this._frustum}updateMatrices(t){const e=this.camera;Eo.setFromMatrixPosition(t.matrixWorld),e.position.copy(Eo),yo.setFromMatrixPosition(t.target.matrixWorld),e.lookAt(yo),e.updateMatrixWorld(),this._updateMatrix(e,this.matrix,this._frustum)}_updateMatrix(t,e,n,r){_s.multiplyMatrices(t.projectionMatrix,t.matrixWorldInverse),n.setFromProjectionMatrix(_s,t.coordinateSystem,t.reversedDepth);const s=this._frameExtents,a=r?r.z/s.x:1,o=r?r.w/s.y:1,c=r?r.x/s.x:0,l=r?r.y/s.y:0;t.coordinateSystem===zi||t.reversedDepth?e.set(.5*a,0,0,.5*a+c,0,.5*o,0,.5*o+l,0,0,1,0,0,0,0,1):e.set(.5*a,0,0,.5*a+c,0,.5*o,0,.5*o+l,0,0,.5,.5,0,0,0,1),e.multiply(_s)}getViewport(t){return this._viewports[t]}getFrameExtents(){return this._frameExtents}dispose(){this.map&&this.map.dispose(),this.mapPass&&this.mapPass.dispose()}copy(t){return this.camera=t.camera.clone(),this.intensity=t.intensity,this.bias=t.bias,this.radius=t.radius,this.autoUpdate=t.autoUpdate,this.needsUpdate=t.needsUpdate,this.normalBias=t.normalBias,this.blurSamples=t.blurSamples,this.mapSize.copy(t.mapSize),this.biasNode=t.biasNode,this}clone(){return new this.constructor().copy(this)}toJSON(){const t={};return t.intensity=this.intensity,t.bias=this.bias,t.normalBias=this.normalBias,t.radius=this.radius,t.blurSamples=this.blurSamples,t.mapSize=this.mapSize.toArray(),t.camera=this.camera.toJSON(!1).object,delete t.camera.matrix,t}}const _r=new I,gr=new Ln,nn=new I;class Il extends Se{constructor(){super(),this.isCamera=!0,this.type="Camera",this.matrixWorldInverse=new ee,this.projectionMatrix=new ee,this.projectionMatrixInverse=new ee,this.coordinateSystem=an,this._reversedDepth=!1}get reversedDepth(){return this._reversedDepth}copy(t,e){return super.copy(t,e),this.matrixWorldInverse.copy(t.matrixWorldInverse),this.projectionMatrix.copy(t.projectionMatrix),this.projectionMatrixInverse.copy(t.projectionMatrixInverse),this.coordinateSystem=t.coordinateSystem,this}getWorldDirection(t){return super.getWorldDirection(t).negate()}updateMatrixWorld(t){super.updateMatrixWorld(t),this.matrixWorld.decompose(_r,gr,nn),nn.x===1&&nn.y===1&&nn.z===1?this.matrixWorldInverse.copy(this.matrixWorld).invert():this.matrixWorldInverse.compose(_r,gr,nn.set(1,1,1)).invert()}updateWorldMatrix(t,e,n=!1){super.updateWorldMatrix(t,e,n),this.matrixWorld.decompose(_r,gr,nn),nn.x===1&&nn.y===1&&nn.z===1?this.matrixWorldInverse.copy(this.matrixWorld).invert():this.matrixWorldInverse.compose(_r,gr,nn.set(1,1,1)).invert()}clone(){return new this.constructor().copy(this)}}const Pn=new I,bo=new Ct,To=new Ct;class qe extends Il{constructor(t=50,e=1,n=.1,r=2e3){super(),this.isPerspectiveCamera=!0,this.type="PerspectiveCamera",this.fov=t,this.zoom=1,this.near=n,this.far=r,this.focus=10,this.aspect=e,this.view=null,this.filmGauge=35,this.filmOffset=0,this.updateProjectionMatrix()}copy(t,e){return super.copy(t,e),this.fov=t.fov,this.zoom=t.zoom,this.near=t.near,this.far=t.far,this.focus=t.focus,this.aspect=t.aspect,this.view=t.view===null?null:Object.assign({},t.view),this.filmGauge=t.filmGauge,this.filmOffset=t.filmOffset,this}setFocalLength(t){const e=.5*this.getFilmHeight()/t;this.fov=Gi*2*Math.atan(e),this.updateProjectionMatrix()}getFocalLength(){const t=Math.tan(_i*.5*this.fov);return .5*this.getFilmHeight()/t}getEffectiveFOV(){return Gi*2*Math.atan(Math.tan(_i*.5*this.fov)/this.zoom)}getFilmWidth(){return this.filmGauge*Math.min(this.aspect,1)}getFilmHeight(){return this.filmGauge/Math.max(this.aspect,1)}getViewBounds(t,e,n){Pn.set(-1,-1,.5).applyMatrix4(this.projectionMatrixInverse),e.set(Pn.x,Pn.y).multiplyScalar(-t/Pn.z),Pn.set(1,1,.5).applyMatrix4(this.projectionMatrixInverse),n.set(Pn.x,Pn.y).multiplyScalar(-t/Pn.z)}getViewSize(t,e){return this.getViewBounds(t,bo,To),e.subVectors(To,bo)}setViewOffset(t,e,n,r,s,a){this.aspect=t/e,this.view===null&&(this.view={enabled:!0,fullWidth:1,fullHeight:1,offsetX:0,offsetY:0,width:1,height:1}),this.view.enabled=!0,this.view.fullWidth=t,this.view.fullHeight=e,this.view.offsetX=n,this.view.offsetY=r,this.view.width=s,this.view.height=a,this.updateProjectionMatrix()}clearViewOffset(){this.view!==null&&(this.view.enabled=!1),this.updateProjectionMatrix()}updateProjectionMatrix(){const t=this.near;let e=t*Math.tan(_i*.5*this.fov)/this.zoom,n=2*e,r=this.aspect*n,s=-.5*r;const a=this.view;if(this.view!==null&&this.view.enabled){const c=a.fullWidth,l=a.fullHeight;s+=a.offsetX*r/c,e-=a.offsetY*n/l,r*=a.width/c,n*=a.height/l}const o=this.filmOffset;o!==0&&(s+=t*o/this.getFilmWidth()),this.projectionMatrix.makePerspective(s,s+r,e,e-n,t,this.far,this.coordinateSystem,this.reversedDepth),this.projectionMatrixInverse.copy(this.projectionMatrix).invert()}toJSON(t){const e=super.toJSON(t);return e.object.fov=this.fov,e.object.zoom=this.zoom,e.object.near=this.near,e.object.far=this.far,e.object.focus=this.focus,e.object.aspect=this.aspect,this.view!==null&&(e.object.view=Object.assign({},this.view)),e.object.filmGauge=this.filmGauge,e.object.filmOffset=this.filmOffset,e}}class Ra extends Il{constructor(t=-1,e=1,n=1,r=-1,s=.1,a=2e3){super(),this.isOrthographicCamera=!0,this.type="OrthographicCamera",this.zoom=1,this.view=null,this.left=t,this.right=e,this.top=n,this.bottom=r,this.near=s,this.far=a,this.updateProjectionMatrix()}copy(t,e){return super.copy(t,e),this.left=t.left,this.right=t.right,this.top=t.top,this.bottom=t.bottom,this.near=t.near,this.far=t.far,this.zoom=t.zoom,this.view=t.view===null?null:Object.assign({},t.view),this}setViewOffset(t,e,n,r,s,a){this.view===null&&(this.view={enabled:!0,fullWidth:1,fullHeight:1,offsetX:0,offsetY:0,width:1,height:1}),this.view.enabled=!0,this.view.fullWidth=t,this.view.fullHeight=e,this.view.offsetX=n,this.view.offsetY=r,this.view.width=s,this.view.height=a,this.updateProjectionMatrix()}clearViewOffset(){this.view!==null&&(this.view.enabled=!1),this.updateProjectionMatrix()}updateProjectionMatrix(){const t=(this.right-this.left)/(2*this.zoom),e=(this.top-this.bottom)/(2*this.zoom),n=(this.right+this.left)/2,r=(this.top+this.bottom)/2;let s=n-t,a=n+t,o=r+e,c=r-e;if(this.view!==null&&this.view.enabled){const l=(this.right-this.left)/this.view.fullWidth/this.zoom,d=(this.top-this.bottom)/this.view.fullHeight/this.zoom;s+=l*this.view.offsetX,a=s+l*this.view.width,o-=d*this.view.offsetY,c=o-d*this.view.height}this.projectionMatrix.makeOrthographic(s,a,o,c,this.near,this.far,this.coordinateSystem,this.reversedDepth),this.projectionMatrixInverse.copy(this.projectionMatrix).invert()}toJSON(t){const e=super.toJSON(t);return e.object.zoom=this.zoom,e.object.left=this.left,e.object.right=this.right,e.object.top=this.top,e.object.bottom=this.bottom,e.object.near=this.near,e.object.far=this.far,this.view!==null&&(e.object.view=Object.assign({},this.view)),e}}class Ph extends Ch{constructor(){super(new Ra(-5,5,5,-5,.5,500)),this.isDirectionalLightShadow=!0}}class e_ extends wa{constructor(t,e){super(t,e),this.isDirectionalLight=!0,this.type="DirectionalLight",this.position.copy(Se.DEFAULT_UP),this.updateMatrix(),this.target=new Se,this.shadow=new Ph}dispose(){super.dispose(),this.shadow.dispose()}copy(t){return super.copy(t),this.target=t.target.clone(),this.shadow=t.shadow.clone(),this}toJSON(t){const e=super.toJSON(t);return e.object.shadow=this.shadow.toJSON(),e.object.target=this.target.uuid,e}}class n_ extends wa{constructor(t,e){super(t,e),this.isAmbientLight=!0,this.type="AmbientLight"}}class i_ extends Ne{constructor(){super(),this.isInstancedBufferGeometry=!0,this.type="InstancedBufferGeometry",this.instanceCount=1/0}copy(t){return super.copy(t),this.instanceCount=t.instanceCount,this}toJSON(){const t=super.toJSON();return t.instanceCount=this.instanceCount,t.isInstancedBufferGeometry=!0,t}}const ai=-90,oi=1;class Dh extends Se{constructor(t,e,n){super(),this.type="CubeCamera",this.renderTarget=n,this.coordinateSystem=null,this.activeMipmapLevel=0;const r=new qe(ai,oi,t,e);r.layers=this.layers,this.add(r);const s=new qe(ai,oi,t,e);s.layers=this.layers,this.add(s);const a=new qe(ai,oi,t,e);a.layers=this.layers,this.add(a);const o=new qe(ai,oi,t,e);o.layers=this.layers,this.add(o);const c=new qe(ai,oi,t,e);c.layers=this.layers,this.add(c);const l=new qe(ai,oi,t,e);l.layers=this.layers,this.add(l)}updateCoordinateSystem(){const t=this.coordinateSystem,e=this.children.concat(),[n,r,s,a,o,c]=e;for(const l of e)this.remove(l);if(t===an)n.up.set(0,1,0),n.lookAt(1,0,0),r.up.set(0,1,0),r.lookAt(-1,0,0),s.up.set(0,0,-1),s.lookAt(0,1,0),a.up.set(0,0,1),a.lookAt(0,-1,0),o.up.set(0,1,0),o.lookAt(0,0,1),c.up.set(0,1,0),c.lookAt(0,0,-1);else if(t===zi)n.up.set(0,-1,0),n.lookAt(-1,0,0),r.up.set(0,-1,0),r.lookAt(1,0,0),s.up.set(0,0,1),s.lookAt(0,1,0),a.up.set(0,0,-1),a.lookAt(0,-1,0),o.up.set(0,-1,0),o.lookAt(0,0,1),c.up.set(0,-1,0),c.lookAt(0,0,-1);else throw new Error("THREE.CubeCamera.updateCoordinateSystem(): Invalid coordinate system: "+t);for(const l of e)this.add(l),l.updateMatrixWorld()}update(t,e){this.parent===null&&this.updateMatrixWorld();const{renderTarget:n,activeMipmapLevel:r}=this;this.coordinateSystem!==t.coordinateSystem&&(this.coordinateSystem=t.coordinateSystem,this.updateCoordinateSystem());const[s,a,o,c,l,d]=this.children,p=t.getRenderTarget(),h=t.getActiveCubeFace(),_=t.getActiveMipmapLevel(),v=t.xr.enabled;t.xr.enabled=!1;const M=n.texture.generateMipmaps;n.texture.generateMipmaps=!1;let m=!1;t.isWebGLRenderer===!0?m=t.state.buffers.depth.getReversed():m=t.reversedDepthBuffer,t.setRenderTarget(n,0,r),m&&t.autoClear===!1&&t.clearDepth(),t.render(e,s),t.setRenderTarget(n,1,r),m&&t.autoClear===!1&&t.clearDepth(),t.render(e,a),t.setRenderTarget(n,2,r),m&&t.autoClear===!1&&t.clearDepth(),t.render(e,o),t.setRenderTarget(n,3,r),m&&t.autoClear===!1&&t.clearDepth(),t.render(e,c),t.setRenderTarget(n,4,r),m&&t.autoClear===!1&&t.clearDepth(),t.render(e,l),n.texture.generateMipmaps=M,t.setRenderTarget(n,5,r),m&&t.autoClear===!1&&t.clearDepth(),t.render(e,d),t.setRenderTarget(p,h,_),t.xr.enabled=v,n.texture.needsPMREMUpdate=!0}}class Lh extends qe{constructor(t=[]){super(),this.isArrayCamera=!0,this.isMultiViewCamera=!1,this.cameras=t}}class r_ extends dh{constructor(t,e,n=1){super(t,e),this.isInstancedInterleavedBuffer=!0,this.meshPerAttribute=n}copy(t){return super.copy(t),this.meshPerAttribute=t.meshPerAttribute,this}clone(t){const e=super.clone(t);return e.meshPerAttribute=this.meshPerAttribute,e}toJSON(t){const e=super.toJSON(t);return e.isInstancedInterleavedBuffer=!0,e.meshPerAttribute=this.meshPerAttribute,e}}const Ao=new ee;class s_{constructor(t,e,n=0,r=1/0){this.ray=new Br(t,e),this.near=n,this.far=r,this.camera=null,this.layers=new Ta,this.params={Mesh:{},Line:{threshold:1},LOD:{},Points:{threshold:1},Sprite:{}}}set(t,e){this.ray.set(t,e)}setFromCamera(t,e){e.isPerspectiveCamera?(this.ray.origin.setFromMatrixPosition(e.matrixWorld),this.ray.direction.set(t.x,t.y,.5).unproject(e).sub(this.ray.origin).normalize(),this.camera=e):e.isOrthographicCamera?(this.ray.origin.set(t.x,t.y,e.projectionMatrix.elements[14]).unproject(e),this.ray.direction.set(0,0,-1).transformDirection(e.matrixWorld),this.camera=e):Wt("Raycaster: Unsupported camera type: "+e.type)}setFromXRController(t){return Ao.identity().extractRotation(t.matrixWorld),this.ray.origin.setFromMatrixPosition(t.matrixWorld),this.ray.direction.set(0,0,-1).applyMatrix4(Ao),this}intersectObject(t,e=!0,n=[]){return ua(t,this,n,e),n.sort(wo),n}intersectObjects(t,e=!0,n=[]){for(let r=0,s=t.length;r<s;r++)ua(t[r],this,n,e);return n.sort(wo),n}}function wo(i,t){return i.distance-t.distance}function ua(i,t,e,n){let r=!0;if(i.layers.test(t.layers)&&i.raycast(t,e)===!1&&(r=!1),r===!0&&n===!0){const s=i.children;for(let a=0,o=s.length;a<o;a++)ua(s[a],t,e,!0)}}class Ro{constructor(t=1,e=0,n=0){this.radius=t,this.phi=e,this.theta=n}set(t,e,n){return this.radius=t,this.phi=e,this.theta=n,this}copy(t){return this.radius=t.radius,this.phi=t.phi,this.theta=t.theta,this}makeSafe(){return this.phi=Lt(this.phi,1e-6,Math.PI-1e-6),this}setFromVector3(t){return this.setFromCartesianCoords(t.x,t.y,t.z)}setFromCartesianCoords(t,e,n){return this.radius=Math.sqrt(t*t+e*e+n*n),this.radius===0?(this.theta=0,this.phi=0):(this.theta=Math.atan2(t,n),this.phi=Math.acos(Lt(e/this.radius,-1,1))),this}clone(){return new this.constructor().copy(this)}}const Ua=class Ua{constructor(t,e,n,r){this.elements=[1,0,0,1],t!==void 0&&this.set(t,e,n,r)}identity(){return this.set(1,0,0,1),this}fromArray(t,e=0){for(let n=0;n<4;n++)this.elements[n]=t[n+e];return this}set(t,e,n,r){const s=this.elements;return s[0]=t,s[2]=e,s[1]=n,s[3]=r,this}};Ua.prototype.isMatrix2=!0;let Co=Ua;const Po=new I,xr=new I,li=new I,ci=new I,gs=new I,Ih=new I,Uh=new I;class a_{constructor(t=new I,e=new I){this.start=t,this.end=e}set(t,e){return this.start.copy(t),this.end.copy(e),this}copy(t){return this.start.copy(t.start),this.end.copy(t.end),this}getCenter(t){return t.addVectors(this.start,this.end).multiplyScalar(.5)}delta(t){return t.subVectors(this.end,this.start)}distanceSq(){return this.start.distanceToSquared(this.end)}distance(){return this.start.distanceTo(this.end)}at(t,e){return this.delta(e).multiplyScalar(t).add(this.start)}closestPointToPointParameter(t,e){Po.subVectors(t,this.start),xr.subVectors(this.end,this.start);const n=xr.dot(xr);if(n===0)return 0;let s=xr.dot(Po)/n;return e&&(s=Lt(s,0,1)),s}closestPointToPoint(t,e,n){const r=this.closestPointToPointParameter(t,e);return this.delta(n).multiplyScalar(r).add(this.start)}distanceSqToLine3(t,e=Ih,n=Uh){const r=10000000000000001e-32;let s,a;const o=this.start,c=t.start,l=this.end,d=t.end;li.subVectors(l,o),ci.subVectors(d,c),gs.subVectors(o,c);const p=li.dot(li),h=ci.dot(ci),_=ci.dot(gs);if(p<=r&&h<=r)return e.copy(o),n.copy(c),e.sub(n),e.dot(e);if(p<=r)s=0,a=_/h,a=Lt(a,0,1);else{const v=li.dot(gs);if(h<=r)a=0,s=Lt(-v/p,0,1);else{const M=li.dot(ci),m=p*h-M*M;m!==0?s=Lt((M*_-v*h)/m,0,1):s=0,a=(M*s+_)/h,a<0?(a=0,s=Lt(-v/p,0,1)):a>1&&(a=1,s=Lt((M-v)/p,0,1))}}return e.copy(o).addScaledVector(li,s),n.copy(c).addScaledVector(ci,a),e.distanceToSquared(n)}applyMatrix4(t){return this.start.applyMatrix4(t),this.end.applyMatrix4(t),this}equals(t){return t.start.equals(this.start)&&t.end.equals(this.end)}clone(){return new this.constructor().copy(this)}}class Nh extends Un{constructor(t,e=null){super(),this.object=t,this.domElement=e,this.enabled=!0,this.state=-1,this.keys={},this.mouseButtons={LEFT:null,MIDDLE:null,RIGHT:null},this.touches={ONE:null,TWO:null}}connect(t){this.domElement!==null&&this.disconnect(),this.domElement=t}disconnect(){}dispose(){}update(){}}function Do(i,t,e,n){const r=Fh(n);switch(e){case gl:return i*t;case ga:return i*t/r.components*r.byteLength;case xa:return i*t/r.components*r.byteLength;case Xn:return i*t*2/r.components*r.byteLength;case va:return i*t*2/r.components*r.byteLength;case xl:return i*t*3/r.components*r.byteLength;case $e:return i*t*4/r.components*r.byteLength;case Sa:return i*t*4/r.components*r.byteLength;case yr:case br:return Math.floor((i+3)/4)*Math.floor((t+3)/4)*8;case Tr:case Ar:return Math.floor((i+3)/4)*Math.floor((t+3)/4)*16;case Ns:case Os:return Math.max(i,16)*Math.max(t,8)/4;case Us:case Fs:return Math.max(i,8)*Math.max(t,8)/2;case Bs:case zs:case Hs:case Vs:return Math.floor((i+3)/4)*Math.floor((t+3)/4)*8;case Gs:case Rr:case ks:return Math.floor((i+3)/4)*Math.floor((t+3)/4)*16;case Ws:return Math.floor((i+3)/4)*Math.floor((t+3)/4)*16;case Xs:return Math.floor((i+4)/5)*Math.floor((t+3)/4)*16;case Ys:return Math.floor((i+4)/5)*Math.floor((t+4)/5)*16;case qs:return Math.floor((i+5)/6)*Math.floor((t+4)/5)*16;case Zs:return Math.floor((i+5)/6)*Math.floor((t+5)/6)*16;case Ks:return Math.floor((i+7)/8)*Math.floor((t+4)/5)*16;case $s:return Math.floor((i+7)/8)*Math.floor((t+5)/6)*16;case Js:return Math.floor((i+7)/8)*Math.floor((t+7)/8)*16;case Qs:return Math.floor((i+9)/10)*Math.floor((t+4)/5)*16;case js:return Math.floor((i+9)/10)*Math.floor((t+5)/6)*16;case ta:return Math.floor((i+9)/10)*Math.floor((t+7)/8)*16;case ea:return Math.floor((i+9)/10)*Math.floor((t+9)/10)*16;case na:return Math.floor((i+11)/12)*Math.floor((t+9)/10)*16;case ia:return Math.floor((i+11)/12)*Math.floor((t+11)/12)*16;case ra:case sa:case aa:return Math.ceil(i/4)*Math.ceil(t/4)*16;case oa:case la:return Math.ceil(i/4)*Math.ceil(t/4)*8;case Cr:case ca:return Math.ceil(i/4)*Math.ceil(t/4)*16}throw new Error(`Unable to determine texture byte length for ${e} format.`)}function Fh(i){switch(i){case Be:case fl:return{byteLength:1,components:1};case Oi:case pl:case cn:return{byteLength:2,components:1};case ma:case _a:return{byteLength:2,components:4};case ln:case pa:case Ke:return{byteLength:4,components:1};case ml:case _l:return{byteLength:4,components:3}}throw new Error(`THREE.TextureUtils: Unknown texture type ${i}.`)}typeof __THREE_DEVTOOLS__<"u"&&__THREE_DEVTOOLS__.dispatchEvent(new CustomEvent("register",{detail:{revision:"186"}}));typeof window<"u"&&(window.__THREE__?Rt("WARNING: Multiple instances of Three.js being imported."):window.__THREE__="186");/**
 * @license
 * Copyright 2010-2026 Three.js Authors
 * SPDX-License-Identifier: MIT
 */function Ul(){let i=null,t=!1,e=null,n=null;function r(s,a){n=i.requestAnimationFrame(r),e(s,a)}return{start:function(){t!==!0&&e!==null&&i!==null&&(n=i.requestAnimationFrame(r),t=!0)},stop:function(){i!==null&&i.cancelAnimationFrame(n),t=!1},setAnimationLoop:function(s){e=s},setContext:function(s){i=s}}}function Oh(i){const t=new WeakMap;function e(o,c){const l=o.array,d=o.usage,p=l.byteLength,h=i.createBuffer();i.bindBuffer(c,h),i.bufferData(c,l,d),o.onUploadCallback();let _;if(l instanceof Float32Array)_=i.FLOAT;else if(typeof Float16Array<"u"&&l instanceof Float16Array)_=i.HALF_FLOAT;else if(l instanceof Uint16Array)o.isFloat16BufferAttribute?_=i.HALF_FLOAT:_=i.UNSIGNED_SHORT;else if(l instanceof Int16Array)_=i.SHORT;else if(l instanceof Uint32Array)_=i.UNSIGNED_INT;else if(l instanceof Int32Array)_=i.INT;else if(l instanceof Int8Array)_=i.BYTE;else if(l instanceof Uint8Array)_=i.UNSIGNED_BYTE;else if(l instanceof Uint8ClampedArray)_=i.UNSIGNED_BYTE;else throw new Error("THREE.WebGLAttributes: Unsupported buffer data format: "+l);return{buffer:h,type:_,bytesPerElement:l.BYTES_PER_ELEMENT,version:o.version,size:p}}function n(o,c,l){const d=c.array,p=c.updateRanges;if(i.bindBuffer(l,o),p.length===0)i.bufferSubData(l,0,d);else{p.sort((_,v)=>_.start-v.start);let h=0;for(let _=1;_<p.length;_++){const v=p[h],M=p[_];M.start<=v.start+v.count+1?v.count=Math.max(v.count,M.start+M.count-v.start):(++h,p[h]=M)}p.length=h+1;for(let _=0,v=p.length;_<v;_++){const M=p[_];i.bufferSubData(l,M.start*d.BYTES_PER_ELEMENT,d,M.start,M.count)}c.clearUpdateRanges()}c.onUploadCallback()}function r(o){return o.isInterleavedBufferAttribute&&(o=o.data),t.get(o)}function s(o){o.isInterleavedBufferAttribute&&(o=o.data);const c=t.get(o);c&&(i.deleteBuffer(c.buffer),t.delete(o))}function a(o,c){if(o.isInterleavedBufferAttribute&&(o=o.data),o.isGLBufferAttribute){const d=t.get(o);(!d||d.version<o.version)&&t.set(o,{buffer:o.buffer,type:o.type,bytesPerElement:o.elementSize,version:o.version});return}const l=t.get(o);if(l===void 0)t.set(o,e(o,c));else if(l.version<o.version){if(l.size!==o.array.byteLength)throw new Error("THREE.WebGLAttributes: The size of the buffer attribute's array buffer does not match the original size. Resizing buffer attributes is not supported.");n(l.buffer,o,c),l.version=o.version}}return{get:r,remove:s,update:a}}var Bh=`#ifdef USE_ALPHAHASH
	if ( diffuseColor.a < getAlphaHashThreshold( vPosition ) ) discard;
#endif`,zh=`#ifdef USE_ALPHAHASH
	const float ALPHA_HASH_SCALE = 0.05;
	float hash2D( vec2 value ) {
		return fract( 1.0e4 * sin( 17.0 * value.x + 0.1 * value.y ) * ( 0.1 + abs( sin( 13.0 * value.y + value.x ) ) ) );
	}
	float hash3D( vec3 value ) {
		return hash2D( vec2( hash2D( value.xy ), value.z ) );
	}
	float getAlphaHashThreshold( vec3 position ) {
		float maxDeriv = max(
			length( dFdx( position.xyz ) ),
			length( dFdy( position.xyz ) )
		);
		float pixScale = 1.0 / ( ALPHA_HASH_SCALE * maxDeriv );
		vec2 pixScales = vec2(
			exp2( floor( log2( pixScale ) ) ),
			exp2( ceil( log2( pixScale ) ) )
		);
		vec2 alpha = vec2(
			hash3D( floor( pixScales.x * position.xyz ) ),
			hash3D( floor( pixScales.y * position.xyz ) )
		);
		float lerpFactor = fract( log2( pixScale ) );
		float x = ( 1.0 - lerpFactor ) * alpha.x + lerpFactor * alpha.y;
		float a = min( lerpFactor, 1.0 - lerpFactor );
		vec3 cases = vec3(
			x * x / ( 2.0 * a * ( 1.0 - a ) ),
			( x - 0.5 * a ) / ( 1.0 - a ),
			1.0 - ( ( 1.0 - x ) * ( 1.0 - x ) / ( 2.0 * a * ( 1.0 - a ) ) )
		);
		float threshold = ( x < ( 1.0 - a ) )
			? ( ( x < a ) ? cases.x : cases.y )
			: cases.z;
		return clamp( threshold , 1.0e-6, 1.0 );
	}
#endif`,Gh=`#ifdef USE_ALPHAMAP
	diffuseColor.a *= texture2D( alphaMap, vAlphaMapUv ).g;
#endif`,Hh=`#ifdef USE_ALPHAMAP
	uniform sampler2D alphaMap;
#endif`,Vh=`#ifdef USE_ALPHATEST
	#ifdef ALPHA_TO_COVERAGE
	diffuseColor.a = smoothstep( alphaTest, alphaTest + fwidth( diffuseColor.a ), diffuseColor.a );
	if ( diffuseColor.a == 0.0 ) discard;
	#else
	if ( diffuseColor.a < alphaTest ) discard;
	#endif
#endif`,kh=`#ifdef USE_ALPHATEST
	uniform float alphaTest;
#endif`,Wh=`#ifdef USE_AOMAP
	float ambientOcclusion = ( texture2D( aoMap, vAoMapUv ).r - 1.0 ) * aoMapIntensity + 1.0;
	reflectedLight.indirectDiffuse *= ambientOcclusion;
	#if defined( USE_CLEARCOAT ) 
		clearcoatSpecularIndirect *= ambientOcclusion;
	#endif
	#if defined( USE_SHEEN ) 
		sheenSpecularIndirect *= ambientOcclusion;
	#endif
	#if defined( USE_ENVMAP ) && defined( STANDARD )
		float dotNV = saturate( dot( geometryNormal, geometryViewDir ) );
		reflectedLight.indirectSpecular *= computeSpecularOcclusion( dotNV, ambientOcclusion, material.roughness );
	#endif
#endif`,Xh=`#ifdef USE_AOMAP
	uniform sampler2D aoMap;
	uniform float aoMapIntensity;
#endif`,Yh=`#ifdef USE_BATCHING
	#if ! defined( GL_ANGLE_multi_draw )
	#define gl_DrawID _gl_DrawID
	uniform int _gl_DrawID;
	#endif
	uniform highp sampler2D batchingTexture;
	uniform highp usampler2D batchingIdTexture;
	mat4 getBatchingMatrix( const in float i ) {
		int size = textureSize( batchingTexture, 0 ).x;
		int j = int( i ) * 4;
		int x = j % size;
		int y = j / size;
		vec4 v1 = texelFetch( batchingTexture, ivec2( x, y ), 0 );
		vec4 v2 = texelFetch( batchingTexture, ivec2( x + 1, y ), 0 );
		vec4 v3 = texelFetch( batchingTexture, ivec2( x + 2, y ), 0 );
		vec4 v4 = texelFetch( batchingTexture, ivec2( x + 3, y ), 0 );
		return mat4( v1, v2, v3, v4 );
	}
	float getIndirectIndex( const in int i ) {
		int size = textureSize( batchingIdTexture, 0 ).x;
		int x = i % size;
		int y = i / size;
		return float( texelFetch( batchingIdTexture, ivec2( x, y ), 0 ).r );
	}
#endif
#ifdef USE_BATCHING_COLOR
	uniform sampler2D batchingColorTexture;
	vec4 getBatchingColor( const in float i ) {
		int size = textureSize( batchingColorTexture, 0 ).x;
		int j = int( i );
		int x = j % size;
		int y = j / size;
		return texelFetch( batchingColorTexture, ivec2( x, y ), 0 );
	}
#endif`,qh=`#ifdef USE_BATCHING
	mat4 batchingMatrix = getBatchingMatrix( getIndirectIndex( gl_DrawID ) );
#endif`,Zh=`vec3 transformed = vec3( position );
#ifdef USE_ALPHAHASH
	vPosition = vec3( position );
#endif`,Kh=`vec3 objectNormal = vec3( normal );
#ifdef USE_TANGENT
	vec3 objectTangent = vec3( tangent.xyz );
#endif`,$h=`float G_BlinnPhong_Implicit( ) {
	return 0.25;
}
float D_BlinnPhong( const in float shininess, const in float dotNH ) {
	return RECIPROCAL_PI * ( shininess * 0.5 + 1.0 ) * pow( dotNH, shininess );
}
vec3 BRDF_BlinnPhong( const in vec3 lightDir, const in vec3 viewDir, const in vec3 normal, const in vec3 specularColor, const in float shininess ) {
	vec3 halfDir = normalize( lightDir + viewDir );
	float dotNH = saturate( dot( normal, halfDir ) );
	float dotVH = saturate( dot( viewDir, halfDir ) );
	vec3 F = F_Schlick( specularColor, 1.0, dotVH );
	float G = G_BlinnPhong_Implicit( );
	float D = D_BlinnPhong( shininess, dotNH );
	return F * ( G * D );
} // validated`,Jh=`#ifdef USE_IRIDESCENCE
	const mat3 XYZ_TO_REC709 = mat3(
		 3.2404542, -0.9692660,  0.0556434,
		-1.5371385,  1.8760108, -0.2040259,
		-0.4985314,  0.0415560,  1.0572252
	);
	vec3 Fresnel0ToIor( vec3 fresnel0 ) {
		vec3 sqrtF0 = sqrt( fresnel0 );
		return ( vec3( 1.0 ) + sqrtF0 ) / ( vec3( 1.0 ) - sqrtF0 );
	}
	vec3 IorToFresnel0( vec3 transmittedIor, float incidentIor ) {
		return pow2( ( transmittedIor - vec3( incidentIor ) ) / ( transmittedIor + vec3( incidentIor ) ) );
	}
	float IorToFresnel0( float transmittedIor, float incidentIor ) {
		return pow2( ( transmittedIor - incidentIor ) / ( transmittedIor + incidentIor ));
	}
	vec3 evalSensitivity( float OPD, vec3 shift ) {
		float phase = 2.0 * PI * OPD * 1.0e-9;
		vec3 val = vec3( 5.4856e-13, 4.4201e-13, 5.2481e-13 );
		vec3 pos = vec3( 1.6810e+06, 1.7953e+06, 2.2084e+06 );
		vec3 var = vec3( 4.3278e+09, 9.3046e+09, 6.6121e+09 );
		vec3 xyz = val * sqrt( 2.0 * PI * var ) * cos( pos * phase + shift ) * exp( - pow2( phase ) * var );
		xyz.x += 9.7470e-14 * sqrt( 2.0 * PI * 4.5282e+09 ) * cos( 2.2399e+06 * phase + shift[ 0 ] ) * exp( - 4.5282e+09 * pow2( phase ) );
		xyz /= 1.0685e-7;
		vec3 rgb = XYZ_TO_REC709 * xyz;
		return rgb;
	}
	vec3 evalIridescence( float outsideIOR, float eta2, float cosTheta1, float thinFilmThickness, vec3 baseF0 ) {
		vec3 I;
		float iridescenceIOR = mix( outsideIOR, eta2, smoothstep( 0.0, 0.03, thinFilmThickness ) );
		float sinTheta2Sq = pow2( outsideIOR / iridescenceIOR ) * ( 1.0 - pow2( cosTheta1 ) );
		float cosTheta2Sq = 1.0 - sinTheta2Sq;
		if ( cosTheta2Sq < 0.0 ) {
			return vec3( 1.0 );
		}
		float cosTheta2 = sqrt( cosTheta2Sq );
		float R0 = IorToFresnel0( iridescenceIOR, outsideIOR );
		float R12 = F_Schlick( R0, 1.0, cosTheta1 );
		float T121 = 1.0 - R12;
		float phi12 = 0.0;
		if ( iridescenceIOR < outsideIOR ) phi12 = PI;
		float phi21 = PI - phi12;
		vec3 baseIOR = Fresnel0ToIor( clamp( baseF0, 0.0, 0.9999 ) );		vec3 R1 = IorToFresnel0( baseIOR, iridescenceIOR );
		vec3 R23 = F_Schlick( R1, 1.0, cosTheta2 );
		vec3 phi23 = vec3( 0.0 );
		if ( baseIOR[ 0 ] < iridescenceIOR ) phi23[ 0 ] = PI;
		if ( baseIOR[ 1 ] < iridescenceIOR ) phi23[ 1 ] = PI;
		if ( baseIOR[ 2 ] < iridescenceIOR ) phi23[ 2 ] = PI;
		float OPD = 2.0 * iridescenceIOR * thinFilmThickness * cosTheta2;
		vec3 phi = vec3( phi21 ) + phi23;
		vec3 R123 = clamp( R12 * R23, 1e-5, 0.9999 );
		vec3 r123 = sqrt( R123 );
		vec3 Rs = pow2( T121 ) * R23 / ( vec3( 1.0 ) - R123 );
		vec3 C0 = R12 + Rs;
		I = C0;
		vec3 Cm = Rs - T121;
		for ( int m = 1; m <= 2; ++ m ) {
			Cm *= r123;
			vec3 Sm = 2.0 * evalSensitivity( float( m ) * OPD, float( m ) * phi );
			I += Cm * Sm;
		}
		return max( I, vec3( 0.0 ) );
	}
#endif`,Qh=`#ifdef USE_BUMPMAP
	uniform sampler2D bumpMap;
	uniform float bumpScale;
	vec2 dHdxy_fwd() {
		vec2 dSTdx = dFdx( vBumpMapUv );
		vec2 dSTdy = dFdy( vBumpMapUv );
		float Hll = bumpScale * texture2D( bumpMap, vBumpMapUv ).x;
		float dBx = bumpScale * texture2D( bumpMap, vBumpMapUv + dSTdx ).x - Hll;
		float dBy = bumpScale * texture2D( bumpMap, vBumpMapUv + dSTdy ).x - Hll;
		return vec2( dBx, dBy );
	}
	vec3 perturbNormalArb( vec3 surf_pos, vec3 surf_norm, vec2 dHdxy, float faceDirection ) {
		vec3 vSigmaX = normalize( dFdx( surf_pos.xyz ) );
		vec3 vSigmaY = normalize( dFdy( surf_pos.xyz ) );
		vec3 vN = surf_norm;
		vec3 R1 = cross( vSigmaY, vN );
		vec3 R2 = cross( vN, vSigmaX );
		float fDet = dot( vSigmaX, R1 ) * faceDirection;
		vec3 vGrad = sign( fDet ) * ( dHdxy.x * R1 + dHdxy.y * R2 );
		return normalize( abs( fDet ) * surf_norm - vGrad );
	}
#endif`,jh=`#if NUM_CLIPPING_PLANES > 0
	vec4 plane;
	#ifdef ALPHA_TO_COVERAGE
		float distanceToPlane, distanceGradient;
		float clipOpacity = 1.0;
		#pragma unroll_loop_start
		for ( int i = 0; i < UNION_CLIPPING_PLANES; i ++ ) {
			plane = clippingPlanes[ i ];
			distanceToPlane = - dot( vClipPosition, plane.xyz ) + plane.w;
			distanceGradient = fwidth( distanceToPlane ) / 2.0;
			clipOpacity *= smoothstep( - distanceGradient, distanceGradient, distanceToPlane );
			if ( clipOpacity == 0.0 ) discard;
		}
		#pragma unroll_loop_end
		#if UNION_CLIPPING_PLANES < NUM_CLIPPING_PLANES
			float unionClipOpacity = 1.0;
			#pragma unroll_loop_start
			for ( int i = UNION_CLIPPING_PLANES; i < NUM_CLIPPING_PLANES; i ++ ) {
				plane = clippingPlanes[ i ];
				distanceToPlane = - dot( vClipPosition, plane.xyz ) + plane.w;
				distanceGradient = fwidth( distanceToPlane ) / 2.0;
				unionClipOpacity *= 1.0 - smoothstep( - distanceGradient, distanceGradient, distanceToPlane );
			}
			#pragma unroll_loop_end
			clipOpacity *= 1.0 - unionClipOpacity;
		#endif
		diffuseColor.a *= clipOpacity;
		if ( diffuseColor.a == 0.0 ) discard;
	#else
		#pragma unroll_loop_start
		for ( int i = 0; i < UNION_CLIPPING_PLANES; i ++ ) {
			plane = clippingPlanes[ i ];
			if ( dot( vClipPosition, plane.xyz ) > plane.w ) discard;
		}
		#pragma unroll_loop_end
		#if UNION_CLIPPING_PLANES < NUM_CLIPPING_PLANES
			bool clipped = true;
			#pragma unroll_loop_start
			for ( int i = UNION_CLIPPING_PLANES; i < NUM_CLIPPING_PLANES; i ++ ) {
				plane = clippingPlanes[ i ];
				clipped = ( dot( vClipPosition, plane.xyz ) > plane.w ) && clipped;
			}
			#pragma unroll_loop_end
			if ( clipped ) discard;
		#endif
	#endif
#endif`,tu=`#if NUM_CLIPPING_PLANES > 0
	varying vec3 vClipPosition;
	uniform vec4 clippingPlanes[ NUM_CLIPPING_PLANES ];
#endif`,eu=`#if NUM_CLIPPING_PLANES > 0
	varying vec3 vClipPosition;
#endif`,nu=`#if NUM_CLIPPING_PLANES > 0
	vClipPosition = - mvPosition.xyz;
#endif`,iu=`#if defined( USE_COLOR ) || defined( USE_COLOR_ALPHA )
	diffuseColor *= vColor;
#endif`,ru=`#if defined( USE_COLOR ) || defined( USE_COLOR_ALPHA )
	varying vec4 vColor;
#endif`,su=`#if defined( USE_COLOR ) || defined( USE_COLOR_ALPHA ) || defined( USE_INSTANCING_COLOR ) || defined( USE_BATCHING_COLOR )
	varying vec4 vColor;
#endif`,au=`#if defined( USE_COLOR ) || defined( USE_COLOR_ALPHA ) || defined( USE_INSTANCING_COLOR ) || defined( USE_BATCHING_COLOR )
	vColor = vec4( 1.0 );
#endif
#ifdef USE_COLOR_ALPHA
	vColor *= color;
#elif defined( USE_COLOR )
	vColor.rgb *= color;
#endif
#ifdef USE_INSTANCING_COLOR
	vColor.rgb *= instanceColor.rgb;
#endif
#ifdef USE_BATCHING_COLOR
	vColor *= getBatchingColor( getIndirectIndex( gl_DrawID ) );
#endif`,ou=`#define PI 3.141592653589793
#define PI2 6.283185307179586
#define PI_HALF 1.5707963267948966
#define RECIPROCAL_PI 0.3183098861837907
#define RECIPROCAL_PI2 0.15915494309189535
#define EPSILON 1e-6
#ifndef saturate
#define saturate( a ) clamp( a, 0.0, 1.0 )
#endif
#define whiteComplement( a ) ( 1.0 - saturate( a ) )
float pow2( const in float x ) { return x*x; }
vec3 pow2( const in vec3 x ) { return x*x; }
float pow3( const in float x ) { return x*x*x; }
float pow4( const in float x ) { float x2 = x*x; return x2*x2; }
float max3( const in vec3 v ) { return max( max( v.x, v.y ), v.z ); }
float average( const in vec3 v ) { return dot( v, vec3( 0.3333333 ) ); }
highp float rand( const in vec2 uv ) {
	const highp float a = 12.9898, b = 78.233, c = 43758.5453;
	highp float dt = dot( uv.xy, vec2( a,b ) ), sn = mod( dt, PI );
	return fract( sin( sn ) * c );
}
#ifdef HIGH_PRECISION
	float precisionSafeLength( vec3 v ) { return length( v ); }
#else
	float precisionSafeLength( vec3 v ) {
		float maxComponent = max3( abs( v ) );
		return length( v / maxComponent ) * maxComponent;
	}
#endif
struct IncidentLight {
	vec3 color;
	vec3 direction;
	bool visible;
};
struct ReflectedLight {
	vec3 directDiffuse;
	vec3 directSpecular;
	vec3 indirectDiffuse;
	vec3 indirectSpecular;
};
#ifdef USE_ALPHAHASH
	varying vec3 vPosition;
#endif
vec3 transformDirection( in vec3 dir, in mat4 matrix ) {
	return normalize( ( matrix * vec4( dir, 0.0 ) ).xyz );
}
#define inverseTransformDirection transformDirectionByInverseViewMatrix
vec3 transformNormalByInverseViewMatrix( in vec3 normal, in mat4 viewMatrix ) {
	return normalize( ( vec4( normal, 0.0 ) * viewMatrix ).xyz );
}
vec3 transformDirectionByInverseViewMatrix( in vec3 dir, in mat4 viewMatrix ) {
	return normalize( ( vec4( dir, 0.0 ) * viewMatrix ).xyz );
}
bool isPerspectiveMatrix( mat4 m ) {
	return m[ 2 ][ 3 ] == - 1.0;
}
vec2 equirectUv( in vec3 dir ) {
	float u = atan( dir.z, dir.x ) * RECIPROCAL_PI2 + 0.5;
	float v = asin( clamp( dir.y, - 1.0, 1.0 ) ) * RECIPROCAL_PI + 0.5;
	return vec2( u, v );
}
vec3 BRDF_Lambert( const in vec3 diffuseColor ) {
	return RECIPROCAL_PI * diffuseColor;
}
vec3 F_Schlick( const in vec3 f0, const in float f90, const in float dotVH ) {
	float fresnel = exp2( ( - 5.55473 * dotVH - 6.98316 ) * dotVH );
	return f0 * ( 1.0 - fresnel ) + ( f90 * fresnel );
}
float F_Schlick( const in float f0, const in float f90, const in float dotVH ) {
	float fresnel = exp2( ( - 5.55473 * dotVH - 6.98316 ) * dotVH );
	return f0 * ( 1.0 - fresnel ) + ( f90 * fresnel );
} // validated`,lu=`#ifdef ENVMAP_TYPE_CUBE_UV
	#define cubeUV_minMipLevel 4.0
	#define cubeUV_minTileSize 16.0
	float getFace( vec3 direction ) {
		vec3 absDirection = abs( direction );
		float face = - 1.0;
		if ( absDirection.x > absDirection.z ) {
			if ( absDirection.x > absDirection.y )
				face = direction.x > 0.0 ? 0.0 : 3.0;
			else
				face = direction.y > 0.0 ? 1.0 : 4.0;
		} else {
			if ( absDirection.z > absDirection.y )
				face = direction.z > 0.0 ? 2.0 : 5.0;
			else
				face = direction.y > 0.0 ? 1.0 : 4.0;
		}
		return face;
	}
	vec2 getUV( vec3 direction, float face ) {
		vec2 uv;
		if ( face == 0.0 ) {
			uv = vec2( direction.z, direction.y ) / abs( direction.x );
		} else if ( face == 1.0 ) {
			uv = vec2( - direction.x, - direction.z ) / abs( direction.y );
		} else if ( face == 2.0 ) {
			uv = vec2( - direction.x, direction.y ) / abs( direction.z );
		} else if ( face == 3.0 ) {
			uv = vec2( - direction.z, direction.y ) / abs( direction.x );
		} else if ( face == 4.0 ) {
			uv = vec2( - direction.x, direction.z ) / abs( direction.y );
		} else {
			uv = vec2( direction.x, direction.y ) / abs( direction.z );
		}
		return 0.5 * ( uv + 1.0 );
	}
	vec3 bilinearCubeUV( sampler2D envMap, vec3 direction, float mipInt ) {
		float face = getFace( direction );
		float filterInt = max( cubeUV_minMipLevel - mipInt, 0.0 );
		mipInt = max( mipInt, cubeUV_minMipLevel );
		float faceSize = exp2( mipInt );
		highp vec2 uv = getUV( direction, face ) * ( faceSize - 2.0 ) + 1.0;
		if ( face > 2.0 ) {
			uv.y += faceSize;
			face -= 3.0;
		}
		uv.x += face * faceSize;
		uv.x += filterInt * 3.0 * cubeUV_minTileSize;
		uv.y += 4.0 * ( exp2( CUBEUV_MAX_MIP ) - faceSize );
		uv.x *= CUBEUV_TEXEL_WIDTH;
		uv.y *= CUBEUV_TEXEL_HEIGHT;
		#ifdef texture2DGradEXT
			return texture2DGradEXT( envMap, uv, vec2( 0.0 ), vec2( 0.0 ) ).rgb;
		#else
			return texture2D( envMap, uv ).rgb;
		#endif
	}
	#define cubeUV_r0 1.0
	#define cubeUV_m0 - 2.0
	#define cubeUV_r1 0.8
	#define cubeUV_m1 - 1.0
	#define cubeUV_r4 0.4
	#define cubeUV_m4 2.0
	#define cubeUV_r5 0.305
	#define cubeUV_m5 3.0
	#define cubeUV_r6 0.21
	#define cubeUV_m6 4.0
	float roughnessToMip( float roughness ) {
		float mip = 0.0;
		if ( roughness >= cubeUV_r1 ) {
			mip = ( cubeUV_r0 - roughness ) * ( cubeUV_m1 - cubeUV_m0 ) / ( cubeUV_r0 - cubeUV_r1 ) + cubeUV_m0;
		} else if ( roughness >= cubeUV_r4 ) {
			mip = ( cubeUV_r1 - roughness ) * ( cubeUV_m4 - cubeUV_m1 ) / ( cubeUV_r1 - cubeUV_r4 ) + cubeUV_m1;
		} else if ( roughness >= cubeUV_r5 ) {
			mip = ( cubeUV_r4 - roughness ) * ( cubeUV_m5 - cubeUV_m4 ) / ( cubeUV_r4 - cubeUV_r5 ) + cubeUV_m4;
		} else if ( roughness >= cubeUV_r6 ) {
			mip = ( cubeUV_r5 - roughness ) * ( cubeUV_m6 - cubeUV_m5 ) / ( cubeUV_r5 - cubeUV_r6 ) + cubeUV_m5;
		} else {
			mip = - 2.0 * log2( 1.16 * roughness );		}
		return mip;
	}
	vec4 textureCubeUV( sampler2D envMap, vec3 sampleDir, float roughness ) {
		float mip = clamp( roughnessToMip( roughness ), cubeUV_m0, CUBEUV_MAX_MIP );
		float mipF = fract( mip );
		float mipInt = floor( mip );
		vec3 color0 = bilinearCubeUV( envMap, sampleDir, mipInt );
		if ( mipF == 0.0 ) {
			return vec4( color0, 1.0 );
		} else {
			vec3 color1 = bilinearCubeUV( envMap, sampleDir, mipInt + 1.0 );
			return vec4( mix( color0, color1, mipF ), 1.0 );
		}
	}
#endif`,cu=`vec3 transformedNormal = objectNormal;
#ifdef USE_TANGENT
	vec3 transformedTangent = objectTangent;
#endif
#ifdef USE_BATCHING
	mat3 bm = mat3( batchingMatrix );
	transformedNormal /= vec3( dot( bm[ 0 ], bm[ 0 ] ), dot( bm[ 1 ], bm[ 1 ] ), dot( bm[ 2 ], bm[ 2 ] ) );
	transformedNormal = bm * transformedNormal;
	#ifdef USE_TANGENT
		transformedTangent = bm * transformedTangent;
	#endif
#endif
#ifdef USE_INSTANCING
	mat3 im = mat3( instanceMatrix );
	transformedNormal /= vec3( dot( im[ 0 ], im[ 0 ] ), dot( im[ 1 ], im[ 1 ] ), dot( im[ 2 ], im[ 2 ] ) );
	transformedNormal = im * transformedNormal;
	#ifdef USE_TANGENT
		transformedTangent = im * transformedTangent;
	#endif
#endif
transformedNormal = normalMatrix * transformedNormal;
#ifdef FLIP_SIDED
	transformedNormal = - transformedNormal;
#endif
#ifdef USE_TANGENT
	transformedTangent = ( modelViewMatrix * vec4( transformedTangent, 0.0 ) ).xyz;
#endif`,hu=`#ifdef USE_DISPLACEMENTMAP
	uniform sampler2D displacementMap;
	uniform float displacementScale;
	uniform float displacementBias;
#endif`,uu=`#ifdef USE_DISPLACEMENTMAP
	transformed += normalize( objectNormal ) * ( texture2D( displacementMap, vDisplacementMapUv ).x * displacementScale + displacementBias );
#endif`,du=`#ifdef USE_EMISSIVEMAP
	vec4 emissiveColor = texture2D( emissiveMap, vEmissiveMapUv );
	#ifdef DECODE_VIDEO_TEXTURE_EMISSIVE
		emissiveColor = sRGBTransferEOTF( emissiveColor );
	#endif
	totalEmissiveRadiance *= emissiveColor.rgb;
#endif`,fu=`#ifdef USE_EMISSIVEMAP
	uniform sampler2D emissiveMap;
#endif`,pu="gl_FragColor = linearToOutputTexel( gl_FragColor );",mu=`vec4 LinearTransferOETF( in vec4 value ) {
	return value;
}
vec4 sRGBTransferEOTF( in vec4 value ) {
	return vec4( mix( pow( value.rgb * 0.9478672986 + vec3( 0.0521327014 ), vec3( 2.4 ) ), value.rgb * 0.0773993808, vec3( lessThanEqual( value.rgb, vec3( 0.04045 ) ) ) ), value.a );
}
vec4 sRGBTransferOETF( in vec4 value ) {
	return vec4( mix( pow( value.rgb, vec3( 0.41666 ) ) * 1.055 - vec3( 0.055 ), value.rgb * 12.92, vec3( lessThanEqual( value.rgb, vec3( 0.0031308 ) ) ) ), value.a );
}`,_u=`#ifdef USE_ENVMAP
	#ifdef ENV_WORLDPOS
		vec3 cameraToFrag;
		if ( isOrthographic ) {
			cameraToFrag = normalize( vec3( - viewMatrix[ 0 ][ 2 ], - viewMatrix[ 1 ][ 2 ], - viewMatrix[ 2 ][ 2 ] ) );
		} else {
			cameraToFrag = normalize( vWorldPosition - cameraPosition );
		}
		vec3 worldNormal = transformNormalByInverseViewMatrix( normal, viewMatrix );
		#ifdef ENVMAP_MODE_REFLECTION
			vec3 reflectVec = reflect( cameraToFrag, worldNormal );
		#else
			vec3 reflectVec = refract( cameraToFrag, worldNormal, refractionRatio );
		#endif
	#else
		vec3 reflectVec = vReflect;
	#endif
	#ifdef ENVMAP_TYPE_CUBE
		vec4 envColor = textureCube( envMap, envMapRotation * reflectVec );
		#ifdef ENVMAP_BLENDING_MULTIPLY
			outgoingLight = mix( outgoingLight, outgoingLight * envColor.xyz, specularStrength * reflectivity );
		#elif defined( ENVMAP_BLENDING_MIX )
			outgoingLight = mix( outgoingLight, envColor.xyz, specularStrength * reflectivity );
		#elif defined( ENVMAP_BLENDING_ADD )
			outgoingLight += envColor.xyz * specularStrength * reflectivity;
		#endif
	#endif
#endif`,gu=`#ifdef USE_ENVMAP
	uniform float envMapIntensity;
	uniform mat3 envMapRotation;
	#ifdef ENVMAP_TYPE_CUBE
		uniform samplerCube envMap;
	#else
		uniform sampler2D envMap;
	#endif
#endif`,xu=`#ifdef USE_ENVMAP
	uniform float reflectivity;
	#if defined( USE_BUMPMAP ) || defined( USE_NORMALMAP ) || defined( PHONG ) || defined( LAMBERT )
		#define ENV_WORLDPOS
	#endif
	#ifdef ENV_WORLDPOS
		varying vec3 vWorldPosition;
		uniform float refractionRatio;
	#else
		varying vec3 vReflect;
	#endif
#endif`,vu=`#ifdef USE_ENVMAP
	#if defined( USE_BUMPMAP ) || defined( USE_NORMALMAP ) || defined( PHONG ) || defined( LAMBERT )
		#define ENV_WORLDPOS
	#endif
	#ifdef ENV_WORLDPOS
		
		varying vec3 vWorldPosition;
	#else
		varying vec3 vReflect;
		uniform float refractionRatio;
	#endif
#endif`,Su=`#ifdef USE_ENVMAP
	#ifdef ENV_WORLDPOS
		vWorldPosition = worldPosition.xyz;
	#else
		vec3 cameraToVertex;
		if ( isOrthographic ) {
			cameraToVertex = normalize( vec3( - viewMatrix[ 0 ][ 2 ], - viewMatrix[ 1 ][ 2 ], - viewMatrix[ 2 ][ 2 ] ) );
		} else {
			cameraToVertex = normalize( worldPosition.xyz - cameraPosition );
		}
		vec3 worldNormal = transformNormalByInverseViewMatrix( transformedNormal, viewMatrix );
		#ifdef ENVMAP_MODE_REFLECTION
			vReflect = reflect( cameraToVertex, worldNormal );
		#else
			vReflect = refract( cameraToVertex, worldNormal, refractionRatio );
		#endif
	#endif
#endif`,Mu=`#ifdef USE_FOG
	vFogDepth = - mvPosition.z;
#endif`,Eu=`#ifdef USE_FOG
	varying float vFogDepth;
#endif`,yu=`#ifdef USE_FOG
	#ifdef FOG_EXP2
		float fogFactor = 1.0 - exp( - fogDensity * fogDensity * vFogDepth * vFogDepth );
	#else
		float fogFactor = smoothstep( fogNear, fogFar, vFogDepth );
	#endif
	gl_FragColor.rgb = mix( gl_FragColor.rgb, fogColor, fogFactor );
#endif`,bu=`#ifdef USE_FOG
	uniform vec3 fogColor;
	varying float vFogDepth;
	#ifdef FOG_EXP2
		uniform float fogDensity;
	#else
		uniform float fogNear;
		uniform float fogFar;
	#endif
#endif`,Tu=`#ifdef USE_GRADIENTMAP
	uniform sampler2D gradientMap;
#endif
vec3 getGradientIrradiance( vec3 normal, vec3 lightDirection ) {
	float dotNL = dot( normal, lightDirection );
	vec2 coord = vec2( dotNL * 0.5 + 0.5, 0.0 );
	#ifdef USE_GRADIENTMAP
		return vec3( texture2D( gradientMap, coord ).r );
	#else
		vec2 fw = fwidth( coord ) * 0.5;
		return mix( vec3( 0.7 ), vec3( 1.0 ), smoothstep( 0.7 - fw.x, 0.7 + fw.x, coord.x ) );
	#endif
}`,Au=`#ifdef USE_LIGHTMAP
	uniform sampler2D lightMap;
	uniform float lightMapIntensity;
#endif`,wu=`LambertMaterial material;
material.diffuseColor = diffuseColor.rgb;
material.specularStrength = specularStrength;`,Ru=`varying vec3 vViewPosition;
struct LambertMaterial {
	vec3 diffuseColor;
	float specularStrength;
};
void RE_Direct_Lambert( const in IncidentLight directLight, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in LambertMaterial material, inout ReflectedLight reflectedLight ) {
	float dotNL = saturate( dot( geometryNormal, directLight.direction ) );
	vec3 irradiance = dotNL * directLight.color;
	reflectedLight.directDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
}
void RE_IndirectDiffuse_Lambert( const in vec3 irradiance, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in LambertMaterial material, inout ReflectedLight reflectedLight ) {
	reflectedLight.indirectDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
}
#define RE_Direct				RE_Direct_Lambert
#define RE_IndirectDiffuse		RE_IndirectDiffuse_Lambert`,Cu=`uniform bool receiveShadow;
uniform vec3 ambientLightColor;
#if defined( USE_LIGHT_PROBES )
	uniform vec3 lightProbe[ 9 ];
#endif
vec3 shGetIrradianceAt( in vec3 normal, in vec3 shCoefficients[ 9 ] ) {
	float x = normal.x, y = normal.y, z = normal.z;
	vec3 result = shCoefficients[ 0 ] * 0.886227;
	result += shCoefficients[ 1 ] * 2.0 * 0.511664 * y;
	result += shCoefficients[ 2 ] * 2.0 * 0.511664 * z;
	result += shCoefficients[ 3 ] * 2.0 * 0.511664 * x;
	result += shCoefficients[ 4 ] * 2.0 * 0.429043 * x * y;
	result += shCoefficients[ 5 ] * 2.0 * 0.429043 * y * z;
	result += shCoefficients[ 6 ] * ( 0.743125 * z * z - 0.247708 );
	result += shCoefficients[ 7 ] * 2.0 * 0.429043 * x * z;
	result += shCoefficients[ 8 ] * 0.429043 * ( x * x - y * y );
	return result;
}
vec3 getLightProbeIrradiance( const in vec3 lightProbe[ 9 ], const in vec3 normal ) {
	vec3 worldNormal = transformNormalByInverseViewMatrix( normal, viewMatrix );
	vec3 irradiance = shGetIrradianceAt( worldNormal, lightProbe );
	return irradiance;
}
vec3 getAmbientLightIrradiance( const in vec3 ambientLightColor ) {
	vec3 irradiance = ambientLightColor;
	return irradiance;
}
float getDistanceAttenuation( const in float lightDistance, const in float cutoffDistance, const in float decayExponent ) {
	float distanceFalloff = 1.0 / max( pow( lightDistance, decayExponent ), 0.01 );
	if ( cutoffDistance > 0.0 ) {
		distanceFalloff *= pow2( saturate( 1.0 - pow4( lightDistance / cutoffDistance ) ) );
	}
	return distanceFalloff;
}
float getSpotAttenuation( const in float coneCosine, const in float penumbraCosine, const in float angleCosine ) {
	return smoothstep( coneCosine, penumbraCosine, angleCosine );
}
#if NUM_SUN_LIGHTS > 0
	struct SunLight {
		vec3 direction;
		vec3 color;
	};
	uniform SunLight sunLights[ NUM_SUN_LIGHTS ];
	void getSunLightInfo( const in SunLight sunLight, out IncidentLight light ) {
		light.color = sunLight.color;
		light.direction = sunLight.direction;
		light.visible = true;
	}
#endif
#if NUM_DIR_LIGHTS > 0
	struct DirectionalLight {
		vec3 direction;
		vec3 color;
	};
	uniform DirectionalLight directionalLights[ NUM_DIR_LIGHTS ];
	void getDirectionalLightInfo( const in DirectionalLight directionalLight, out IncidentLight light ) {
		light.color = directionalLight.color;
		light.direction = directionalLight.direction;
		light.visible = true;
	}
#endif
#if NUM_POINT_LIGHTS > 0
	struct PointLight {
		vec3 position;
		vec3 color;
		float distance;
		float decay;
	};
	uniform PointLight pointLights[ NUM_POINT_LIGHTS ];
	void getPointLightInfo( const in PointLight pointLight, const in vec3 geometryPosition, out IncidentLight light ) {
		vec3 lVector = pointLight.position - geometryPosition;
		light.direction = normalize( lVector );
		float lightDistance = length( lVector );
		light.color = pointLight.color;
		light.color *= getDistanceAttenuation( lightDistance, pointLight.distance, pointLight.decay );
		light.visible = ( light.color != vec3( 0.0 ) );
	}
#endif
#if NUM_SPOT_LIGHTS > 0
	struct SpotLight {
		vec3 position;
		vec3 direction;
		vec3 color;
		float distance;
		float decay;
		float coneCos;
		float penumbraCos;
	};
	uniform SpotLight spotLights[ NUM_SPOT_LIGHTS ];
	void getSpotLightInfo( const in SpotLight spotLight, const in vec3 geometryPosition, out IncidentLight light ) {
		vec3 lVector = spotLight.position - geometryPosition;
		light.direction = normalize( lVector );
		float angleCos = dot( light.direction, spotLight.direction );
		float spotAttenuation = getSpotAttenuation( spotLight.coneCos, spotLight.penumbraCos, angleCos );
		if ( spotAttenuation > 0.0 ) {
			float lightDistance = length( lVector );
			light.color = spotLight.color * spotAttenuation;
			light.color *= getDistanceAttenuation( lightDistance, spotLight.distance, spotLight.decay );
			light.visible = ( light.color != vec3( 0.0 ) );
		} else {
			light.color = vec3( 0.0 );
			light.visible = false;
		}
	}
#endif
#if NUM_RECT_AREA_LIGHTS > 0
	struct RectAreaLight {
		vec3 color;
		vec3 position;
		vec3 halfWidth;
		vec3 halfHeight;
	};
	uniform sampler2D ltc_1;	uniform sampler2D ltc_2;
	uniform RectAreaLight rectAreaLights[ NUM_RECT_AREA_LIGHTS ];
#endif
#if NUM_HEMI_LIGHTS > 0
	struct HemisphereLight {
		vec3 direction;
		vec3 skyColor;
		vec3 groundColor;
	};
	uniform HemisphereLight hemisphereLights[ NUM_HEMI_LIGHTS ];
	vec3 getHemisphereLightIrradiance( const in HemisphereLight hemiLight, const in vec3 normal ) {
		float dotNL = dot( normal, hemiLight.direction );
		float hemiDiffuseWeight = 0.5 * dotNL + 0.5;
		vec3 irradiance = mix( hemiLight.groundColor, hemiLight.skyColor, hemiDiffuseWeight );
		return irradiance;
	}
#endif
#include <lightprobes_pars_fragment>`,Pu=`#ifdef USE_ENVMAP
	vec3 getIBLIrradiance( const in vec3 normal ) {
		#ifdef ENVMAP_TYPE_CUBE_UV
			vec3 worldNormal = transformNormalByInverseViewMatrix( normal, viewMatrix );
			vec4 envMapColor = textureCubeUV( envMap, envMapRotation * worldNormal, 1.0 );
			return PI * envMapColor.rgb * envMapIntensity;
		#else
			return vec3( 0.0 );
		#endif
	}
	vec3 getIBLRadiance( const in vec3 viewDir, const in vec3 normal, const in float roughness ) {
		#ifdef ENVMAP_TYPE_CUBE_UV
			vec3 reflectVec = reflect( - viewDir, normal );
			reflectVec = normalize( mix( reflectVec, normal, pow4( roughness ) ) );
			reflectVec = transformDirectionByInverseViewMatrix( reflectVec, viewMatrix );
			vec4 envMapColor = textureCubeUV( envMap, envMapRotation * reflectVec, roughness );
			return envMapColor.rgb * envMapIntensity;
		#else
			return vec3( 0.0 );
		#endif
	}
	#ifdef USE_RETROREFLECTION
		vec3 getIBLRetroRadiance( const in vec3 viewDir, const in vec3 normal, const in float roughness ) {
			#ifdef ENVMAP_TYPE_CUBE_UV
				vec3 retroVec = normalize( mix( viewDir, normal, pow4( roughness ) ) );
				retroVec = transformDirectionByInverseViewMatrix( retroVec, viewMatrix );
				vec4 envMapColor = textureCubeUV( envMap, envMapRotation * retroVec, roughness );
				return envMapColor.rgb * envMapIntensity;
			#else
				return vec3( 0.0 );
			#endif
		}
	#endif
	#ifdef USE_ANISOTROPY
		vec3 getIBLAnisotropyRadiance( const in vec3 viewDir, const in vec3 normal, const in float roughness, const in vec3 bitangent, const in float anisotropy ) {
			#ifdef ENVMAP_TYPE_CUBE_UV
				vec3 bentNormal = cross( bitangent, viewDir );
				bentNormal = normalize( cross( bentNormal, bitangent ) );
				bentNormal = normalize( mix( bentNormal, normal, pow2( pow2( 1.0 - anisotropy * ( 1.0 - roughness ) ) ) ) );
				return getIBLRadiance( viewDir, bentNormal, roughness );
			#else
				return vec3( 0.0 );
			#endif
		}
		#ifdef USE_RETROREFLECTION
			vec3 getIBLAnisotropyRetroRadiance( const in vec3 viewDir, const in vec3 normal, const in float roughness, const in vec3 bitangent, const in float anisotropy ) {
				#ifdef ENVMAP_TYPE_CUBE_UV
					vec3 bentNormal = cross( bitangent, viewDir );
					bentNormal = normalize( cross( bentNormal, bitangent ) );
					bentNormal = normalize( mix( bentNormal, normal, pow2( pow2( 1.0 - anisotropy * ( 1.0 - roughness ) ) ) ) );
					return getIBLRetroRadiance( viewDir, bentNormal, roughness );
				#else
					return vec3( 0.0 );
				#endif
			}
		#endif
	#endif
#endif`,Du=`ToonMaterial material;
material.diffuseColor = diffuseColor.rgb;`,Lu=`varying vec3 vViewPosition;
struct ToonMaterial {
	vec3 diffuseColor;
};
void RE_Direct_Toon( const in IncidentLight directLight, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in ToonMaterial material, inout ReflectedLight reflectedLight ) {
	vec3 irradiance = getGradientIrradiance( geometryNormal, directLight.direction ) * directLight.color;
	reflectedLight.directDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
}
void RE_IndirectDiffuse_Toon( const in vec3 irradiance, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in ToonMaterial material, inout ReflectedLight reflectedLight ) {
	reflectedLight.indirectDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
}
#define RE_Direct				RE_Direct_Toon
#define RE_IndirectDiffuse		RE_IndirectDiffuse_Toon`,Iu=`BlinnPhongMaterial material;
material.diffuseColor = diffuseColor.rgb;
material.specularColor = specular;
material.specularShininess = shininess;
material.specularStrength = specularStrength;`,Uu=`varying vec3 vViewPosition;
struct BlinnPhongMaterial {
	vec3 diffuseColor;
	vec3 specularColor;
	float specularShininess;
	float specularStrength;
};
void RE_Direct_BlinnPhong( const in IncidentLight directLight, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in BlinnPhongMaterial material, inout ReflectedLight reflectedLight ) {
	float dotNL = saturate( dot( geometryNormal, directLight.direction ) );
	vec3 irradiance = dotNL * directLight.color;
	reflectedLight.directDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
	reflectedLight.directSpecular += irradiance * BRDF_BlinnPhong( directLight.direction, geometryViewDir, geometryNormal, material.specularColor, material.specularShininess ) * material.specularStrength;
}
void RE_IndirectDiffuse_BlinnPhong( const in vec3 irradiance, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in BlinnPhongMaterial material, inout ReflectedLight reflectedLight ) {
	reflectedLight.indirectDiffuse += irradiance * BRDF_Lambert( material.diffuseColor );
}
#define RE_Direct				RE_Direct_BlinnPhong
#define RE_IndirectDiffuse		RE_IndirectDiffuse_BlinnPhong`,Nu=`PhysicalMaterial material;
material.diffuseColor = diffuseColor.rgb;
material.diffuseContribution = diffuseColor.rgb * ( 1.0 - metalnessFactor );
material.metalness = metalnessFactor;
vec3 dxy = max( abs( dFdx( nonPerturbedNormal ) ), abs( dFdy( nonPerturbedNormal ) ) );
float geometryRoughness = max( max( dxy.x, dxy.y ), dxy.z );
material.roughness = max( roughnessFactor, 0.0525 );material.roughness += geometryRoughness;
material.roughness = min( material.roughness, 1.0 );
#ifdef IOR
	material.ior = ior;
	#ifdef USE_SPECULAR
		float specularIntensityFactor = specularIntensity;
		vec3 specularColorFactor = specularColor;
		#ifdef USE_SPECULAR_COLORMAP
			specularColorFactor *= texture2D( specularColorMap, vSpecularColorMapUv ).rgb;
		#endif
		#ifdef USE_SPECULAR_INTENSITYMAP
			specularIntensityFactor *= texture2D( specularIntensityMap, vSpecularIntensityMapUv ).a;
		#endif
		material.specularF90 = mix( specularIntensityFactor, 1.0, metalnessFactor );
	#else
		float specularIntensityFactor = 1.0;
		vec3 specularColorFactor = vec3( 1.0 );
		material.specularF90 = 1.0;
	#endif
	material.specularColor = min( pow2( ( material.ior - 1.0 ) / ( material.ior + 1.0 ) ) * specularColorFactor, vec3( 1.0 ) ) * specularIntensityFactor;
	material.specularColorBlended = mix( material.specularColor, diffuseColor.rgb, metalnessFactor );
#else
	material.specularColor = vec3( 0.04 );
	material.specularColorBlended = mix( material.specularColor, diffuseColor.rgb, metalnessFactor );
	material.specularF90 = 1.0;
#endif
#ifdef USE_CLEARCOAT
	material.clearcoat = clearcoat;
	material.clearcoatRoughness = clearcoatRoughness;
	material.clearcoatF0 = vec3( 0.04 );
	material.clearcoatF90 = 1.0;
	#ifdef USE_CLEARCOATMAP
		material.clearcoat *= texture2D( clearcoatMap, vClearcoatMapUv ).x;
	#endif
	#ifdef USE_CLEARCOAT_ROUGHNESSMAP
		material.clearcoatRoughness *= texture2D( clearcoatRoughnessMap, vClearcoatRoughnessMapUv ).y;
	#endif
	material.clearcoat = saturate( material.clearcoat );	material.clearcoatRoughness = max( material.clearcoatRoughness, 0.0525 );
	material.clearcoatRoughness += geometryRoughness;
	material.clearcoatRoughness = min( material.clearcoatRoughness, 1.0 );
#endif
#ifdef USE_DISPERSION
	material.dispersion = dispersion;
#endif
#ifdef USE_RETROREFLECTION
	material.retroreflectivity = retroreflectivity;
#endif
#ifdef USE_IRIDESCENCE
	material.iridescence = iridescence;
	material.iridescenceIOR = iridescenceIOR;
	#ifdef USE_IRIDESCENCEMAP
		material.iridescence *= texture2D( iridescenceMap, vIridescenceMapUv ).r;
	#endif
	#ifdef USE_IRIDESCENCE_THICKNESSMAP
		material.iridescenceThickness = (iridescenceThicknessMaximum - iridescenceThicknessMinimum) * texture2D( iridescenceThicknessMap, vIridescenceThicknessMapUv ).g + iridescenceThicknessMinimum;
	#else
		material.iridescenceThickness = iridescenceThicknessMaximum;
	#endif
#endif
#ifdef USE_SHEEN
	material.sheenColor = sheenColor;
	#ifdef USE_SHEEN_COLORMAP
		material.sheenColor *= texture2D( sheenColorMap, vSheenColorMapUv ).rgb;
	#endif
	material.sheenRoughness = clamp( sheenRoughness, 0.0001, 1.0 );
	#ifdef USE_SHEEN_ROUGHNESSMAP
		material.sheenRoughness *= texture2D( sheenRoughnessMap, vSheenRoughnessMapUv ).a;
	#endif
#endif
#ifdef USE_ANISOTROPY
	#ifdef USE_ANISOTROPYMAP
		mat2 anisotropyMat = mat2( anisotropyVector.x, anisotropyVector.y, - anisotropyVector.y, anisotropyVector.x );
		vec3 anisotropyPolar = texture2D( anisotropyMap, vAnisotropyMapUv ).rgb;
		vec2 anisotropyV = anisotropyMat * normalize( 2.0 * anisotropyPolar.rg - vec2( 1.0 ) ) * anisotropyPolar.b;
	#else
		vec2 anisotropyV = anisotropyVector;
	#endif
	material.anisotropy = length( anisotropyV );
	if( material.anisotropy == 0.0 ) {
		anisotropyV = vec2( 1.0, 0.0 );
	} else {
		anisotropyV /= material.anisotropy;
		material.anisotropy = saturate( material.anisotropy );
	}
	material.alphaT = mix( pow2( material.roughness ), 1.0, pow2( material.anisotropy ) );
	material.anisotropyT = tbn[ 0 ] * anisotropyV.x + tbn[ 1 ] * anisotropyV.y;
	material.anisotropyB = tbn[ 1 ] * anisotropyV.x - tbn[ 0 ] * anisotropyV.y;
#endif`,Fu=`uniform sampler2D dfgLUT;
struct PhysicalMaterial {
	vec3 diffuseColor;
	vec3 diffuseContribution;
	vec3 specularColor;
	vec3 specularColorBlended;
	float roughness;
	float metalness;
	float specularF90;
	float dispersion;
	vec2 dfg;
	vec3 multiScatteringCompensation;
	#ifdef USE_RETROREFLECTION
		float retroreflectivity;
	#endif
	#ifdef USE_CLEARCOAT
		float clearcoat;
		float clearcoatRoughness;
		vec3 clearcoatF0;
		float clearcoatF90;
	#endif
	#ifdef USE_IRIDESCENCE
		float iridescence;
		float iridescenceIOR;
		float iridescenceThickness;
		vec3 iridescenceFresnel;
		vec3 iridescenceF0Dielectric;
		vec3 iridescenceF0Metallic;
	#endif
	#ifdef USE_SHEEN
		vec3 sheenColor;
		float sheenRoughness;
	#endif
	#ifdef IOR
		float ior;
	#endif
	#ifdef USE_TRANSMISSION
		float transmission;
		float transmissionAlpha;
		float thickness;
		float attenuationDistance;
		vec3 attenuationColor;
	#endif
	#ifdef USE_ANISOTROPY
		float anisotropy;
		float alphaT;
		vec3 anisotropyT;
		vec3 anisotropyB;
	#endif
};
vec3 clearcoatSpecularDirect = vec3( 0.0 );
vec3 clearcoatSpecularIndirect = vec3( 0.0 );
vec3 sheenSpecularDirect = vec3( 0.0 );
vec3 sheenSpecularIndirect = vec3(0.0 );
vec3 Schlick_to_F0( const in vec3 f, const in float f90, const in float dotVH ) {
    float x = clamp( 1.0 - dotVH, 0.0, 1.0 );
    float x2 = x * x;
    float x5 = clamp( x * x2 * x2, 0.0, 0.9999 );
    return ( f - vec3( f90 ) * x5 ) / ( 1.0 - x5 );
}
float V_GGX_SmithCorrelated( const in float alpha, const in float dotNL, const in float dotNV ) {
	float a2 = pow2( alpha );
	float gv = dotNL * sqrt( a2 + ( 1.0 - a2 ) * pow2( dotNV ) );
	float gl = dotNV * sqrt( a2 + ( 1.0 - a2 ) * pow2( dotNL ) );
	return 0.5 / max( gv + gl, EPSILON );
}
float D_GGX( const in float alpha, const in float dotNH ) {
	float a2 = pow2( alpha );
	float denom = pow2( dotNH ) * ( a2 - 1.0 ) + 1.0;
	return RECIPROCAL_PI * a2 / pow2( denom );
}
#ifdef USE_ANISOTROPY
	float V_GGX_SmithCorrelated_Anisotropic( const in float alphaT, const in float alphaB, const in float dotTV, const in float dotBV, const in float dotTL, const in float dotBL, const in float dotNV, const in float dotNL ) {
		float gv = dotNL * length( vec3( alphaT * dotTV, alphaB * dotBV, dotNV ) );
		float gl = dotNV * length( vec3( alphaT * dotTL, alphaB * dotBL, dotNL ) );
		return 0.5 / max( gv + gl, EPSILON );
	}
	float D_GGX_Anisotropic( const in float alphaT, const in float alphaB, const in float dotNH, const in float dotTH, const in float dotBH ) {
		float a2 = alphaT * alphaB;
		highp vec3 v = vec3( alphaB * dotTH, alphaT * dotBH, a2 * dotNH );
		highp float v2 = dot( v, v );
		float w2 = a2 / v2;
		return RECIPROCAL_PI * a2 * pow2 ( w2 );
	}
#endif
#ifdef USE_CLEARCOAT
	vec3 BRDF_GGX_Clearcoat( const in vec3 lightDir, const in vec3 viewDir, const in vec3 normal, const in PhysicalMaterial material) {
		vec3 f0 = material.clearcoatF0;
		float f90 = material.clearcoatF90;
		float roughness = material.clearcoatRoughness;
		float alpha = pow2( roughness );
		vec3 halfDir = normalize( lightDir + viewDir );
		float dotNL = saturate( dot( normal, lightDir ) );
		float dotNV = saturate( dot( normal, viewDir ) );
		float dotNH = saturate( dot( normal, halfDir ) );
		float dotVH = saturate( dot( viewDir, halfDir ) );
		vec3 F = F_Schlick( f0, f90, dotVH );
		float V = V_GGX_SmithCorrelated( alpha, dotNL, dotNV );
		float D = D_GGX( alpha, dotNH );
		return F * ( V * D );
	}
#endif
vec3 BRDF_GGX( const in vec3 lightDir, const in vec3 viewDir, const in vec3 normal, const in PhysicalMaterial material ) {
	vec3 f0 = material.specularColorBlended;
	float f90 = material.specularF90;
	float roughness = material.roughness;
	float alpha = pow2( roughness );
	vec3 halfDir = normalize( lightDir + viewDir );
	float dotNL = saturate( dot( normal, lightDir ) );
	float dotNV = saturate( dot( normal, viewDir ) );
	float dotNH = saturate( dot( normal, halfDir ) );
	float dotVH = saturate( dot( viewDir, halfDir ) );
	vec3 F = F_Schlick( f0, f90, dotVH );
	#ifdef USE_IRIDESCENCE
		F = mix( F, material.iridescenceFresnel, material.iridescence );
	#endif
	#ifdef USE_ANISOTROPY
		float dotTL = dot( material.anisotropyT, lightDir );
		float dotTV = dot( material.anisotropyT, viewDir );
		float dotTH = dot( material.anisotropyT, halfDir );
		float dotBL = dot( material.anisotropyB, lightDir );
		float dotBV = dot( material.anisotropyB, viewDir );
		float dotBH = dot( material.anisotropyB, halfDir );
		float V = V_GGX_SmithCorrelated_Anisotropic( material.alphaT, alpha, dotTV, dotBV, dotTL, dotBL, dotNV, dotNL );
		float D = D_GGX_Anisotropic( material.alphaT, alpha, dotNH, dotTH, dotBH );
	#else
		float V = V_GGX_SmithCorrelated( alpha, dotNL, dotNV );
		float D = D_GGX( alpha, dotNH );
	#endif
	return F * ( V * D );
}
vec2 LTC_Uv( const in vec3 N, const in vec3 V, const in float roughness ) {
	const float LUT_SIZE = 64.0;
	const float LUT_SCALE = ( LUT_SIZE - 1.0 ) / LUT_SIZE;
	const float LUT_BIAS = 0.5 / LUT_SIZE;
	float dotNV = saturate( dot( N, V ) );
	vec2 uv = vec2( roughness, sqrt( 1.0 - dotNV ) );
	uv = uv * LUT_SCALE + LUT_BIAS;
	return uv;
}
float LTC_ClippedSphereFormFactor( const in vec3 f ) {
	float l = length( f );
	return max( ( l * l + f.z ) / ( l + 1.0 ), 0.0 );
}
vec3 LTC_EdgeVectorFormFactor( const in vec3 v1, const in vec3 v2 ) {
	float x = dot( v1, v2 );
	float y = abs( x );
	float a = 0.8543985 + ( 0.4965155 + 0.0145206 * y ) * y;
	float b = 3.4175940 + ( 4.1616724 + y ) * y;
	float v = a / b;
	float theta_sintheta = ( x > 0.0 ) ? v : 0.5 * inversesqrt( max( 1.0 - x * x, 1e-7 ) ) - v;
	return cross( v1, v2 ) * theta_sintheta;
}
vec3 LTC_Evaluate( const in vec3 N, const in vec3 V, const in vec3 P, const in mat3 mInv, const in vec3 rectCoords[ 4 ] ) {
	vec3 v1 = rectCoords[ 1 ] - rectCoords[ 0 ];
	vec3 v2 = rectCoords[ 3 ] - rectCoords[ 0 ];
	vec3 lightNormal = cross( v1, v2 );
	if( dot( lightNormal, P - rectCoords[ 0 ] ) < 0.0 ) return vec3( 0.0 );
	vec3 T1, T2;
	T1 = normalize( V - N * dot( V, N ) );
	T2 = - cross( N, T1 );
	mat3 mat = mInv * transpose( mat3( T1, T2, N ) );
	vec3 coords[ 4 ];
	coords[ 0 ] = mat * ( rectCoords[ 0 ] - P );
	coords[ 1 ] = mat * ( rectCoords[ 1 ] - P );
	coords[ 2 ] = mat * ( rectCoords[ 2 ] - P );
	coords[ 3 ] = mat * ( rectCoords[ 3 ] - P );
	coords[ 0 ] = normalize( coords[ 0 ] );
	coords[ 1 ] = normalize( coords[ 1 ] );
	coords[ 2 ] = normalize( coords[ 2 ] );
	coords[ 3 ] = normalize( coords[ 3 ] );
	vec3 vectorFormFactor = vec3( 0.0 );
	vectorFormFactor += LTC_EdgeVectorFormFactor( coords[ 0 ], coords[ 1 ] );
	vectorFormFactor += LTC_EdgeVectorFormFactor( coords[ 1 ], coords[ 2 ] );
	vectorFormFactor += LTC_EdgeVectorFormFactor( coords[ 2 ], coords[ 3 ] );
	vectorFormFactor += LTC_EdgeVectorFormFactor( coords[ 3 ], coords[ 0 ] );
	float result = LTC_ClippedSphereFormFactor( vectorFormFactor );
	return vec3( result );
}
#if defined( USE_SHEEN )
float D_Charlie( float roughness, float dotNH ) {
	float alpha = pow2( roughness );
	float invAlpha = 1.0 / alpha;
	float cos2h = dotNH * dotNH;
	float sin2h = max( 1.0 - cos2h, 0.0078125 );
	return ( 2.0 + invAlpha ) * pow( sin2h, invAlpha * 0.5 ) / ( 2.0 * PI );
}
float V_Neubelt( float dotNV, float dotNL ) {
	return saturate( 1.0 / ( 4.0 * ( dotNL + dotNV - dotNL * dotNV ) ) );
}
vec3 BRDF_Sheen( const in vec3 lightDir, const in vec3 viewDir, const in vec3 normal, vec3 sheenColor, const in float sheenRoughness ) {
	vec3 halfDir = normalize( lightDir + viewDir );
	float dotNL = saturate( dot( normal, lightDir ) );
	float dotNV = saturate( dot( normal, viewDir ) );
	float dotNH = saturate( dot( normal, halfDir ) );
	float D = D_Charlie( sheenRoughness, dotNH );
	float V = V_Neubelt( dotNV, dotNL );
	return sheenColor * ( D * V );
}
#endif
float IBLSheenBRDF( const in vec3 normal, const in vec3 viewDir, const in float roughness ) {
	float dotNV = saturate( dot( normal, viewDir ) );
	float r2 = roughness * roughness;
	float rInv = 1.0 / ( roughness + 0.1 );
	float a = -1.9362 + 1.0678 * roughness + 0.4573 * r2 - 0.8469 * rInv;
	float b = -0.6014 + 0.5538 * roughness - 0.4670 * r2 - 0.1255 * rInv;
	float DG = exp( a * dotNV + b );
	return saturate( DG );
}
vec3 EnvironmentBRDF( const in vec3 normal, const in vec3 viewDir, const in vec3 specularColor, const in float specularF90, const in float roughness ) {
	float dotNV = saturate( dot( normal, viewDir ) );
	vec2 fab = texture2D( dfgLUT, vec2( roughness, dotNV ) ).rg;
	return specularColor * fab.x + specularF90 * fab.y;
}
#ifdef USE_IRIDESCENCE
void computeMultiscatteringIridescence( const in vec2 fab, const in vec3 specularColor, const in float specularF90, const in float iridescence, const in vec3 iridescenceF0, inout vec3 singleScatter, inout vec3 multiScatter ) {
#else
void computeMultiscattering( const in vec2 fab, const in vec3 specularColor, const in float specularF90, inout vec3 singleScatter, inout vec3 multiScatter ) {
#endif
	#ifdef USE_IRIDESCENCE
		vec3 Fr = mix( specularColor, iridescenceF0, iridescence );
	#else
		vec3 Fr = specularColor;
	#endif
	vec3 FssEss = Fr * fab.x + specularF90 * fab.y;
	float Ess = fab.x + fab.y;
	float Ems = 1.0 - Ess;
	vec3 Favg = Fr + ( 1.0 - Fr ) * 0.047619;	vec3 Fms = FssEss * Favg / ( 1.0 - Ems * Favg );
	singleScatter += FssEss;
	multiScatter += Fms * Ems;
}
#if NUM_RECT_AREA_LIGHTS > 0
	void RE_Direct_RectArea_Physical( const in RectAreaLight rectAreaLight, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in PhysicalMaterial material, inout ReflectedLight reflectedLight ) {
		vec3 normal = geometryNormal;
		vec3 viewDir = geometryViewDir;
		vec3 position = geometryPosition;
		vec3 lightPos = rectAreaLight.position;
		vec3 halfWidth = rectAreaLight.halfWidth;
		vec3 halfHeight = rectAreaLight.halfHeight;
		vec3 lightColor = rectAreaLight.color;
		float roughness = material.roughness;
		vec3 rectCoords[ 4 ];
		rectCoords[ 0 ] = lightPos + halfWidth - halfHeight;		rectCoords[ 1 ] = lightPos - halfWidth - halfHeight;
		rectCoords[ 2 ] = lightPos - halfWidth + halfHeight;
		rectCoords[ 3 ] = lightPos + halfWidth + halfHeight;
		vec2 uv = LTC_Uv( normal, viewDir, roughness );
		vec4 t1 = texture2D( ltc_1, uv );
		vec4 t2 = texture2D( ltc_2, uv );
		mat3 mInv = mat3(
			vec3( t1.x, 0, t1.y ),
			vec3(    0, 1,    0 ),
			vec3( t1.z, 0, t1.w )
		);
		vec3 fresnel = ( material.specularColorBlended * t2.x + ( material.specularF90 - material.specularColorBlended ) * t2.y );
		reflectedLight.directSpecular += lightColor * fresnel * LTC_Evaluate( normal, viewDir, position, mInv, rectCoords );
		reflectedLight.directDiffuse += lightColor * material.diffuseContribution * LTC_Evaluate( normal, viewDir, position, mat3( 1.0 ), rectCoords );
		#ifdef USE_CLEARCOAT
			vec3 Ncc = geometryClearcoatNormal;
			vec2 uvClearcoat = LTC_Uv( Ncc, viewDir, material.clearcoatRoughness );
			vec4 t1Clearcoat = texture2D( ltc_1, uvClearcoat );
			vec4 t2Clearcoat = texture2D( ltc_2, uvClearcoat );
			mat3 mInvClearcoat = mat3(
				vec3( t1Clearcoat.x, 0, t1Clearcoat.y ),
				vec3(             0, 1,             0 ),
				vec3( t1Clearcoat.z, 0, t1Clearcoat.w )
			);
			vec3 fresnelClearcoat = material.clearcoatF0 * t2Clearcoat.x + ( material.clearcoatF90 - material.clearcoatF0 ) * t2Clearcoat.y;
			clearcoatSpecularDirect += lightColor * fresnelClearcoat * LTC_Evaluate( Ncc, viewDir, position, mInvClearcoat, rectCoords );
		#endif
	}
#endif
void RE_Direct_Physical( const in IncidentLight directLight, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in PhysicalMaterial material, inout ReflectedLight reflectedLight ) {
	float dotNL = saturate( dot( geometryNormal, directLight.direction ) );
	vec3 irradiance = dotNL * directLight.color;
	#ifdef USE_CLEARCOAT
		float dotNLcc = saturate( dot( geometryClearcoatNormal, directLight.direction ) );
		vec3 ccIrradiance = dotNLcc * directLight.color;
		clearcoatSpecularDirect += ccIrradiance * BRDF_GGX_Clearcoat( directLight.direction, geometryViewDir, geometryClearcoatNormal, material );
	#endif
	#ifdef USE_SHEEN
 
 		sheenSpecularDirect += irradiance * BRDF_Sheen( directLight.direction, geometryViewDir, geometryNormal, material.sheenColor, material.sheenRoughness );
 
 		float sheenAlbedoV = IBLSheenBRDF( geometryNormal, geometryViewDir, material.sheenRoughness );
 		float sheenAlbedoL = IBLSheenBRDF( geometryNormal, directLight.direction, material.sheenRoughness );
 
 		float sheenEnergyComp = 1.0 - max3( material.sheenColor ) * max( sheenAlbedoV, sheenAlbedoL );
 
 		irradiance *= sheenEnergyComp;
 
 	#endif
	vec3 specularBRDF = BRDF_GGX( directLight.direction, geometryViewDir, geometryNormal, material );
	#ifdef USE_RETROREFLECTION
		vec3 retroViewDir = reflect( - geometryViewDir, geometryNormal );
		vec3 retroSpecularBRDF = BRDF_GGX( directLight.direction, retroViewDir, geometryNormal, material );
		specularBRDF = mix( specularBRDF, retroSpecularBRDF, saturate( material.retroreflectivity ) );
	#endif
	reflectedLight.directSpecular += irradiance * specularBRDF * material.multiScatteringCompensation;
	vec3 halfDir = normalize( directLight.direction + geometryViewDir );
	float dotVH = saturate( dot( geometryViewDir, halfDir ) );
	vec3 F = F_Schlick( material.specularColor, material.specularF90, dotVH );
	#ifdef USE_RETROREFLECTION
		vec3 retroHalfDir = normalize( directLight.direction + retroViewDir );
		float dotRetroVH = saturate( dot( retroViewDir, retroHalfDir ) );
		vec3 retroF = F_Schlick( material.specularColor, material.specularF90, dotRetroVH );
		F = mix( F, retroF, saturate( material.retroreflectivity ) );
	#endif
	reflectedLight.directDiffuse += irradiance * BRDF_Lambert( material.diffuseContribution ) * ( 1.0 - F );
}
void RE_IndirectDiffuse_Physical( const in vec3 irradiance, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in PhysicalMaterial material, inout ReflectedLight reflectedLight ) {
	vec3 singleScattering = vec3( 0.0 );
	vec3 multiScattering = vec3( 0.0 );
	#ifdef USE_IRIDESCENCE
		computeMultiscatteringIridescence( material.dfg, material.specularColor, material.specularF90, material.iridescence, material.iridescenceF0Dielectric, singleScattering, multiScattering );
	#else
		computeMultiscattering( material.dfg, material.specularColor, material.specularF90, singleScattering, multiScattering );
	#endif
	vec3 diffuse = irradiance * BRDF_Lambert( material.diffuseContribution ) * ( 1.0 - singleScattering - multiScattering );
	#ifdef USE_SHEEN
		float sheenAlbedo = IBLSheenBRDF( geometryNormal, geometryViewDir, material.sheenRoughness );
		sheenSpecularIndirect += irradiance * material.sheenColor * sheenAlbedo * RECIPROCAL_PI;
		float sheenEnergyComp = 1.0 - max3( material.sheenColor ) * sheenAlbedo;
		diffuse *= sheenEnergyComp;
	#endif
	reflectedLight.indirectDiffuse += diffuse;
}
void RE_IndirectSpecular_Physical( const in vec3 radiance, const in vec3 irradiance, const in vec3 clearcoatRadiance, const in vec3 geometryPosition, const in vec3 geometryNormal, const in vec3 geometryViewDir, const in vec3 geometryClearcoatNormal, const in PhysicalMaterial material, inout ReflectedLight reflectedLight) {
	#ifdef USE_CLEARCOAT
		clearcoatSpecularIndirect += clearcoatRadiance * EnvironmentBRDF( geometryClearcoatNormal, geometryViewDir, material.clearcoatF0, material.clearcoatF90, material.clearcoatRoughness );
	#endif
	#ifdef USE_SHEEN
		sheenSpecularIndirect += irradiance * material.sheenColor * IBLSheenBRDF( geometryNormal, geometryViewDir, material.sheenRoughness ) * RECIPROCAL_PI;
 	#endif
	vec3 singleScatteringDielectric = vec3( 0.0 );
	vec3 multiScatteringDielectric = vec3( 0.0 );
	vec3 singleScatteringMetallic = vec3( 0.0 );
	vec3 multiScatteringMetallic = vec3( 0.0 );
	#ifdef USE_IRIDESCENCE
		computeMultiscatteringIridescence( material.dfg, material.specularColor, material.specularF90, material.iridescence, material.iridescenceF0Dielectric, singleScatteringDielectric, multiScatteringDielectric );
		computeMultiscatteringIridescence( material.dfg, material.diffuseColor, material.specularF90, material.iridescence, material.iridescenceF0Metallic, singleScatteringMetallic, multiScatteringMetallic );
	#else
		computeMultiscattering( material.dfg, material.specularColor, material.specularF90, singleScatteringDielectric, multiScatteringDielectric );
		computeMultiscattering( material.dfg, material.diffuseColor, material.specularF90, singleScatteringMetallic, multiScatteringMetallic );
	#endif
	vec3 singleScattering = mix( singleScatteringDielectric, singleScatteringMetallic, material.metalness );
	vec3 multiScattering = mix( multiScatteringDielectric, multiScatteringMetallic, material.metalness );
	vec3 totalScatteringDielectric = singleScatteringDielectric + multiScatteringDielectric;
	vec3 diffuse = material.diffuseContribution * ( 1.0 - totalScatteringDielectric );
	vec3 cosineWeightedIrradiance = irradiance * RECIPROCAL_PI;
	vec3 indirectSpecular = radiance * singleScattering;
	indirectSpecular += multiScattering * cosineWeightedIrradiance;
	vec3 indirectDiffuse = diffuse * cosineWeightedIrradiance;
	#ifdef USE_SHEEN
		float sheenAlbedo = IBLSheenBRDF( geometryNormal, geometryViewDir, material.sheenRoughness );
		float sheenEnergyComp = 1.0 - max3( material.sheenColor ) * sheenAlbedo;
		indirectSpecular *= sheenEnergyComp;
		indirectDiffuse *= sheenEnergyComp;
	#endif
	reflectedLight.indirectSpecular += indirectSpecular;
	reflectedLight.indirectDiffuse += indirectDiffuse;
}
#define RE_Direct				RE_Direct_Physical
#define RE_Direct_RectArea		RE_Direct_RectArea_Physical
#define RE_IndirectDiffuse		RE_IndirectDiffuse_Physical
#define RE_IndirectSpecular		RE_IndirectSpecular_Physical
float computeSpecularOcclusion( const in float dotNV, const in float ambientOcclusion, const in float roughness ) {
	return saturate( pow( dotNV + ambientOcclusion, exp2( - 16.0 * roughness - 1.0 ) ) - 1.0 + ambientOcclusion );
}`,Ou=`
vec3 geometryPosition = - vViewPosition;
vec3 geometryNormal = normal;
vec3 geometryViewDir = ( isOrthographic ) ? vec3( 0, 0, 1 ) : normalize( vViewPosition );
vec3 geometryClearcoatNormal = vec3( 0.0 );
#ifdef USE_CLEARCOAT
	geometryClearcoatNormal = clearcoatNormal;
#endif
#ifdef USE_IRIDESCENCE
	float dotNVi = saturate( dot( normal, geometryViewDir ) );
	if ( material.iridescenceThickness == 0.0 ) {
		material.iridescence = 0.0;
	} else {
		material.iridescence = saturate( material.iridescence );
	}
	if ( material.iridescence > 0.0 ) {
		vec3 iridescenceFresnelDielectric = evalIridescence( 1.0, material.iridescenceIOR, dotNVi, material.iridescenceThickness, material.specularColor );
		vec3 iridescenceFresnelMetallic = evalIridescence( 1.0, material.iridescenceIOR, dotNVi, material.iridescenceThickness, material.diffuseColor );
		material.iridescenceFresnel = mix( iridescenceFresnelDielectric, iridescenceFresnelMetallic, material.metalness );
		material.iridescenceF0Dielectric = Schlick_to_F0( iridescenceFresnelDielectric, 1.0, dotNVi );
		material.iridescenceF0Metallic = Schlick_to_F0( iridescenceFresnelMetallic, 1.0, dotNVi );
	}
#endif
#ifdef STANDARD
	float dotNVms = saturate( dot( geometryNormal, geometryViewDir ) );
	material.dfg = texture2D( dfgLUT, vec2( material.roughness, dotNVms ) ).rg;
	#if ( NUM_SUN_LIGHTS > 0 || NUM_DIR_LIGHTS > 0 || NUM_POINT_LIGHTS > 0 || NUM_SPOT_LIGHTS > 0 )
		float EssMs = material.dfg.x + material.dfg.y;
		material.multiScatteringCompensation = 1.0 + material.specularColorBlended * ( 1.0 / EssMs - 1.0 );
	#endif
#endif
IncidentLight directLight;
#if ( NUM_POINT_LIGHTS > 0 ) && defined( RE_Direct )
	PointLight pointLight;
	#if defined( USE_SHADOWMAP ) && NUM_POINT_LIGHT_SHADOWS > 0
	PointLightShadow pointLightShadow;
	#endif
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_POINT_LIGHTS; i ++ ) {
		pointLight = pointLights[ i ];
		getPointLightInfo( pointLight, geometryPosition, directLight );
		#if defined( USE_SHADOWMAP ) && ( UNROLLED_LOOP_INDEX < NUM_POINT_LIGHT_SHADOWS ) && ( defined( SHADOWMAP_TYPE_PCF ) || defined( SHADOWMAP_TYPE_BASIC ) )
		pointLightShadow = pointLightShadows[ i ];
		directLight.color *= ( directLight.visible && receiveShadow ) ? getPointShadow( pointShadowMap[ i ], pointLightShadow.shadowMapSize, pointLightShadow.shadowIntensity, pointLightShadow.shadowBias, pointLightShadow.shadowRadius, vPointShadowCoord[ i ], pointLightShadow.shadowCameraNear, pointLightShadow.shadowCameraFar ) : 1.0;
		#endif
		RE_Direct( directLight, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
	}
	#pragma unroll_loop_end
#endif
#if ( NUM_SPOT_LIGHTS > 0 ) && defined( RE_Direct )
	SpotLight spotLight;
	vec4 spotColor;
	vec3 spotLightCoord;
	bool inSpotLightMap;
	#if defined( USE_SHADOWMAP ) && NUM_SPOT_LIGHT_SHADOWS > 0
	SpotLightShadow spotLightShadow;
	#endif
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_SPOT_LIGHTS; i ++ ) {
		spotLight = spotLights[ i ];
		getSpotLightInfo( spotLight, geometryPosition, directLight );
		#if ( UNROLLED_LOOP_INDEX < NUM_SPOT_LIGHT_SHADOWS_WITH_MAPS )
		#define SPOT_LIGHT_MAP_INDEX UNROLLED_LOOP_INDEX
		#elif ( UNROLLED_LOOP_INDEX < NUM_SPOT_LIGHT_SHADOWS )
		#define SPOT_LIGHT_MAP_INDEX NUM_SPOT_LIGHT_MAPS
		#else
		#define SPOT_LIGHT_MAP_INDEX ( UNROLLED_LOOP_INDEX - NUM_SPOT_LIGHT_SHADOWS + NUM_SPOT_LIGHT_SHADOWS_WITH_MAPS )
		#endif
		#if ( SPOT_LIGHT_MAP_INDEX < NUM_SPOT_LIGHT_MAPS )
			spotLightCoord = vSpotLightCoord[ i ].xyz / vSpotLightCoord[ i ].w;
			inSpotLightMap = all( lessThan( abs( spotLightCoord * 2. - 1. ), vec3( 1.0 ) ) );
			spotColor = texture2D( spotLightMap[ SPOT_LIGHT_MAP_INDEX ], spotLightCoord.xy );
			directLight.color = inSpotLightMap ? directLight.color * spotColor.rgb : directLight.color;
		#endif
		#undef SPOT_LIGHT_MAP_INDEX
		#if defined( USE_SHADOWMAP ) && ( UNROLLED_LOOP_INDEX < NUM_SPOT_LIGHT_SHADOWS )
		spotLightShadow = spotLightShadows[ i ];
		directLight.color *= ( directLight.visible && receiveShadow ) ? getShadow( spotShadowMap[ i ], spotLightShadow.shadowMapSize, spotLightShadow.shadowIntensity, spotLightShadow.shadowBias, spotLightShadow.shadowRadius, vSpotLightCoord[ i ] ) : 1.0;
		#endif
		RE_Direct( directLight, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
	}
	#pragma unroll_loop_end
#endif
#if ( NUM_SUN_LIGHTS > 0 ) && defined( RE_Direct )
	SunLight sunLight;
	#if defined( USE_SHADOWMAP ) && NUM_SUN_LIGHT_SHADOWS > 0
	SunLightShadow sunLightShadow;
	#endif
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_SUN_LIGHTS; i ++ ) {
		sunLight = sunLights[ i ];
		getSunLightInfo( sunLight, directLight );
		#if defined( USE_SHADOWMAP ) && ( UNROLLED_LOOP_INDEX < NUM_SUN_LIGHT_SHADOWS )
		sunLightShadow = sunLightShadows[ i ];
		directLight.color *= ( directLight.visible && receiveShadow ) ? getSunShadow( sunShadowMap[ i ], sunLightShadow, UNROLLED_LOOP_INDEX ) : 1.0;
		#endif
		RE_Direct( directLight, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
	}
	#pragma unroll_loop_end
#endif
#if ( NUM_DIR_LIGHTS > 0 ) && defined( RE_Direct )
	DirectionalLight directionalLight;
	#if defined( USE_SHADOWMAP ) && NUM_DIR_LIGHT_SHADOWS > 0
	DirectionalLightShadow directionalLightShadow;
	#endif
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_DIR_LIGHTS; i ++ ) {
		directionalLight = directionalLights[ i ];
		getDirectionalLightInfo( directionalLight, directLight );
		#if defined( USE_SHADOWMAP ) && ( UNROLLED_LOOP_INDEX < NUM_DIR_LIGHT_SHADOWS )
		directionalLightShadow = directionalLightShadows[ i ];
		directLight.color *= ( directLight.visible && receiveShadow ) ? getShadow( directionalShadowMap[ i ], directionalLightShadow.shadowMapSize, directionalLightShadow.shadowIntensity, directionalLightShadow.shadowBias, directionalLightShadow.shadowRadius, vDirectionalShadowCoord[ i ] ) : 1.0;
		#endif
		RE_Direct( directLight, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
	}
	#pragma unroll_loop_end
#endif
#if ( NUM_RECT_AREA_LIGHTS > 0 ) && defined( RE_Direct_RectArea )
	RectAreaLight rectAreaLight;
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_RECT_AREA_LIGHTS; i ++ ) {
		rectAreaLight = rectAreaLights[ i ];
		RE_Direct_RectArea( rectAreaLight, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
	}
	#pragma unroll_loop_end
#endif
#if defined( RE_IndirectDiffuse )
	vec3 iblIrradiance = vec3( 0.0 );
	vec3 irradiance = getAmbientLightIrradiance( ambientLightColor );
	#if defined( USE_LIGHT_PROBES )
		irradiance += getLightProbeIrradiance( lightProbe, geometryNormal );
	#endif
	#if ( NUM_HEMI_LIGHTS > 0 )
		#pragma unroll_loop_start
		for ( int i = 0; i < NUM_HEMI_LIGHTS; i ++ ) {
			irradiance += getHemisphereLightIrradiance( hemisphereLights[ i ], geometryNormal );
		}
		#pragma unroll_loop_end
	#endif
	#ifdef USE_LIGHT_PROBES_GRID
		vec3 probeWorldPos = ( ( vec4( geometryPosition, 1.0 ) - viewMatrix[ 3 ] ) * viewMatrix ).xyz;
		vec3 probeWorldNormal = transformNormalByInverseViewMatrix( geometryNormal, viewMatrix );
		irradiance += getLightProbeGridIrradiance( probeWorldPos, probeWorldNormal );
	#endif
#endif
#if defined( RE_IndirectSpecular )
	vec3 radiance = vec3( 0.0 );
	vec3 clearcoatRadiance = vec3( 0.0 );
#endif`,Bu=`#if defined( RE_IndirectDiffuse )
	#ifdef USE_LIGHTMAP
		vec4 lightMapTexel = texture2D( lightMap, vLightMapUv );
		vec3 lightMapIrradiance = lightMapTexel.rgb * lightMapIntensity;
		irradiance += lightMapIrradiance;
	#endif
	#if defined( USE_ENVMAP ) && defined( ENVMAP_TYPE_CUBE_UV )
		#if defined( STANDARD ) || defined( LAMBERT ) || defined( PHONG )
			iblIrradiance += getIBLIrradiance( geometryNormal );
		#endif
	#endif
#endif
#if defined( USE_ENVMAP ) && defined( RE_IndirectSpecular )
	#ifdef USE_ANISOTROPY
		vec3 iblRadiance = getIBLAnisotropyRadiance( geometryViewDir, geometryNormal, material.roughness, material.anisotropyB, material.anisotropy );
	#else
		vec3 iblRadiance = getIBLRadiance( geometryViewDir, geometryNormal, material.roughness );
	#endif
	#ifdef USE_RETROREFLECTION
		#ifdef USE_ANISOTROPY
			vec3 retroIBLRadiance = getIBLAnisotropyRetroRadiance( geometryViewDir, geometryNormal, material.roughness, material.anisotropyB, material.anisotropy );
		#else
			vec3 retroIBLRadiance = getIBLRetroRadiance( geometryViewDir, geometryNormal, material.roughness );
		#endif
		iblRadiance = mix( iblRadiance, retroIBLRadiance, saturate( material.retroreflectivity ) );
	#endif
	radiance += iblRadiance;
	#ifdef USE_CLEARCOAT
		clearcoatRadiance += getIBLRadiance( geometryViewDir, geometryClearcoatNormal, material.clearcoatRoughness );
	#endif
#endif`,zu=`#if defined( RE_IndirectDiffuse )
	#if defined( LAMBERT ) || defined( PHONG )
		irradiance += iblIrradiance;
	#endif
	RE_IndirectDiffuse( irradiance, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
#endif
#if defined( RE_IndirectSpecular )
	RE_IndirectSpecular( radiance, iblIrradiance, clearcoatRadiance, geometryPosition, geometryNormal, geometryViewDir, geometryClearcoatNormal, material, reflectedLight );
#endif`,Gu=`#ifdef USE_LIGHT_PROBES_GRID
uniform highp sampler3D probesSH;
uniform vec3 probesMin;
uniform vec3 probesMax;
uniform vec3 probesResolution;
vec3 getLightProbeGridIrradiance( vec3 worldPos, vec3 worldNormal ) {
	vec3 res = probesResolution;
	vec3 gridRange = probesMax - probesMin;
	vec3 resMinusOne = res - 1.0;
	vec3 probeSpacing = gridRange / resMinusOne;
	vec3 samplePos = worldPos + worldNormal * probeSpacing * 0.5;
	vec3 uvw = clamp( ( samplePos - probesMin ) / gridRange, 0.0, 1.0 );
	uvw = uvw * resMinusOne / res + 0.5 / res;
	float nz          = res.z;
	float paddedSlices = nz + 2.0;
	float atlasDepth  = 7.0 * paddedSlices;
	float uvZBase     = uvw.z * nz + 1.0;
	vec4 s0 = texture( probesSH, vec3( uvw.xy, ( uvZBase                       ) / atlasDepth ) );
	vec4 s1 = texture( probesSH, vec3( uvw.xy, ( uvZBase +       paddedSlices   ) / atlasDepth ) );
	vec4 s2 = texture( probesSH, vec3( uvw.xy, ( uvZBase + 2.0 * paddedSlices   ) / atlasDepth ) );
	vec4 s3 = texture( probesSH, vec3( uvw.xy, ( uvZBase + 3.0 * paddedSlices   ) / atlasDepth ) );
	vec4 s4 = texture( probesSH, vec3( uvw.xy, ( uvZBase + 4.0 * paddedSlices   ) / atlasDepth ) );
	vec4 s5 = texture( probesSH, vec3( uvw.xy, ( uvZBase + 5.0 * paddedSlices   ) / atlasDepth ) );
	vec4 s6 = texture( probesSH, vec3( uvw.xy, ( uvZBase + 6.0 * paddedSlices   ) / atlasDepth ) );
	vec3 c0 = s0.xyz;
	vec3 c1 = vec3( s0.w, s1.xy );
	vec3 c2 = vec3( s1.zw, s2.x );
	vec3 c3 = s2.yzw;
	vec3 c4 = s3.xyz;
	vec3 c5 = vec3( s3.w, s4.xy );
	vec3 c6 = vec3( s4.zw, s5.x );
	vec3 c7 = s5.yzw;
	vec3 c8 = s6.xyz;
	float x = worldNormal.x, y = worldNormal.y, z = worldNormal.z;
	vec3 result = c0 * 0.886227;
	result += c1 * 2.0 * 0.511664 * y;
	result += c2 * 2.0 * 0.511664 * z;
	result += c3 * 2.0 * 0.511664 * x;
	result += c4 * 2.0 * 0.429043 * x * y;
	result += c5 * 2.0 * 0.429043 * y * z;
	result += c6 * ( 0.743125 * z * z - 0.247708 );
	result += c7 * 2.0 * 0.429043 * x * z;
	result += c8 * 0.429043 * ( x * x - y * y );
	return max( result, vec3( 0.0 ) );
}
#endif`,Hu=`#if defined( USE_LOGARITHMIC_DEPTH_BUFFER )
	gl_FragDepth = vIsPerspective == 0.0 ? gl_FragCoord.z : log2( vFragDepth ) * logDepthBufFC * 0.5;
#endif`,Vu=`#if defined( USE_LOGARITHMIC_DEPTH_BUFFER )
	uniform float logDepthBufFC;
	varying float vFragDepth;
	varying float vIsPerspective;
#endif`,ku=`#ifdef USE_LOGARITHMIC_DEPTH_BUFFER
	varying float vFragDepth;
	varying float vIsPerspective;
#endif`,Wu=`#ifdef USE_LOGARITHMIC_DEPTH_BUFFER
	vFragDepth = 1.0 + gl_Position.w;
	vIsPerspective = float( isPerspectiveMatrix( projectionMatrix ) );
#endif`,Xu=`#ifdef USE_MAP
	vec4 sampledDiffuseColor = texture2D( map, vMapUv );
	#ifdef DECODE_VIDEO_TEXTURE
		sampledDiffuseColor = sRGBTransferEOTF( sampledDiffuseColor );
	#endif
	diffuseColor *= sampledDiffuseColor;
#endif`,Yu=`#ifdef USE_MAP
	uniform sampler2D map;
#endif`,qu=`#if defined( USE_MAP ) || defined( USE_ALPHAMAP )
	#if defined( USE_POINTS_UV )
		vec2 uv = vUv;
	#else
		vec2 uv = ( uvTransform * vec3( gl_PointCoord.x, 1.0 - gl_PointCoord.y, 1 ) ).xy;
	#endif
#endif
#ifdef USE_MAP
	diffuseColor *= texture2D( map, uv );
#endif
#ifdef USE_ALPHAMAP
	diffuseColor.a *= texture2D( alphaMap, uv ).g;
#endif`,Zu=`#if defined( USE_POINTS_UV )
	varying vec2 vUv;
#else
	#if defined( USE_MAP ) || defined( USE_ALPHAMAP )
		uniform mat3 uvTransform;
	#endif
#endif
#ifdef USE_MAP
	uniform sampler2D map;
#endif
#ifdef USE_ALPHAMAP
	uniform sampler2D alphaMap;
#endif`,Ku=`float metalnessFactor = metalness;
#ifdef USE_METALNESSMAP
	vec4 texelMetalness = texture2D( metalnessMap, vMetalnessMapUv );
	metalnessFactor *= texelMetalness.b;
#endif`,$u=`#ifdef USE_METALNESSMAP
	uniform sampler2D metalnessMap;
#endif`,Ju=`#ifdef USE_INSTANCING_MORPH
	float morphTargetInfluences[ MORPHTARGETS_COUNT ];
	float morphTargetBaseInfluence = texelFetch( morphTexture, ivec2( 0, gl_InstanceID ), 0 ).r;
	for ( int i = 0; i < MORPHTARGETS_COUNT; i ++ ) {
		morphTargetInfluences[i] =  texelFetch( morphTexture, ivec2( i + 1, gl_InstanceID ), 0 ).r;
	}
#endif`,Qu=`#if defined( USE_MORPHCOLORS )
	vColor *= morphTargetBaseInfluence;
	for ( int i = 0; i < MORPHTARGETS_COUNT; i ++ ) {
		#if defined( USE_COLOR_ALPHA )
			if ( morphTargetInfluences[ i ] != 0.0 ) vColor += getMorph( gl_VertexID, i, 2 ) * morphTargetInfluences[ i ];
		#elif defined( USE_COLOR )
			if ( morphTargetInfluences[ i ] != 0.0 ) vColor += getMorph( gl_VertexID, i, 2 ).rgb * morphTargetInfluences[ i ];
		#endif
	}
#endif`,ju=`#ifdef USE_MORPHNORMALS
	objectNormal *= morphTargetBaseInfluence;
	for ( int i = 0; i < MORPHTARGETS_COUNT; i ++ ) {
		if ( morphTargetInfluences[ i ] != 0.0 ) objectNormal += getMorph( gl_VertexID, i, 1 ).xyz * morphTargetInfluences[ i ];
	}
#endif`,td=`#ifdef USE_MORPHTARGETS
	#ifndef USE_INSTANCING_MORPH
		uniform float morphTargetBaseInfluence;
		uniform float morphTargetInfluences[ MORPHTARGETS_COUNT ];
	#endif
	uniform sampler2DArray morphTargetsTexture;
	uniform ivec2 morphTargetsTextureSize;
	vec4 getMorph( const in int vertexIndex, const in int morphTargetIndex, const in int offset ) {
		int texelIndex = vertexIndex * MORPHTARGETS_TEXTURE_STRIDE + offset;
		int y = texelIndex / morphTargetsTextureSize.x;
		int x = texelIndex - y * morphTargetsTextureSize.x;
		ivec3 morphUV = ivec3( x, y, morphTargetIndex );
		return texelFetch( morphTargetsTexture, morphUV, 0 );
	}
#endif`,ed=`#ifdef USE_MORPHTARGETS
	transformed *= morphTargetBaseInfluence;
	for ( int i = 0; i < MORPHTARGETS_COUNT; i ++ ) {
		if ( morphTargetInfluences[ i ] != 0.0 ) transformed += getMorph( gl_VertexID, i, 0 ).xyz * morphTargetInfluences[ i ];
	}
#endif`,nd=`float faceDirection = gl_FrontFacing ? 1.0 : - 1.0;
#ifdef FLAT_SHADED
	vec3 fdx = dFdx( vViewPosition );
	vec3 fdy = dFdy( vViewPosition );
	vec3 normal = normalize( cross( fdx, fdy ) );
#else
	vec3 normal = normalize( vNormal );
	#ifdef DOUBLE_SIDED
		normal *= faceDirection;
	#endif
#endif
#if defined( USE_NORMALMAP_TANGENTSPACE ) || defined( USE_CLEARCOAT_NORMALMAP ) || defined( USE_ANISOTROPY )
	#ifdef USE_TANGENT
		mat3 tbn = mat3( normalize( vTangent ), normalize( vBitangent ), normal );
	#else
		mat3 tbn = getTangentFrame( - vViewPosition, normal,
		#if defined( USE_NORMALMAP )
			vNormalMapUv
		#elif defined( USE_CLEARCOAT_NORMALMAP )
			vClearcoatNormalMapUv
		#else
			vUv
		#endif
		);
	#endif
	#ifdef DOUBLE_SIDED
		tbn[0] *= faceDirection;
		tbn[1] *= faceDirection;
	#endif
#endif
#ifdef USE_CLEARCOAT_NORMALMAP
	#ifdef USE_TANGENT
		mat3 tbn2 = mat3( normalize( vTangent ), normalize( vBitangent ), normal );
	#else
		mat3 tbn2 = getTangentFrame( - vViewPosition, normal, vClearcoatNormalMapUv );
	#endif
	#ifdef DOUBLE_SIDED
		tbn2[0] *= faceDirection;
		tbn2[1] *= faceDirection;
	#endif
#endif
vec3 nonPerturbedNormal = normal;`,id=`#ifdef USE_NORMALMAP_OBJECTSPACE
	normal = texture2D( normalMap, vNormalMapUv ).xyz * 2.0 - 1.0;
	#ifdef FLIP_SIDED
		normal = - normal;
	#endif
	#ifdef DOUBLE_SIDED
		normal = normal * faceDirection;
	#endif
	normal = normalize( normalMatrix * normal );
#elif defined( USE_NORMALMAP_TANGENTSPACE )
	vec3 mapN = texture2D( normalMap, vNormalMapUv ).xyz * 2.0 - 1.0;
	#if defined( USE_PACKED_NORMALMAP )
		mapN = vec3( mapN.xy, sqrt( saturate( 1.0 - dot( mapN.xy, mapN.xy ) ) ) );
	#endif
	mapN.xy *= normalScale;
	normal = normalize( tbn * mapN );
#elif defined( USE_BUMPMAP )
	normal = perturbNormalArb( - vViewPosition, normal, dHdxy_fwd(), faceDirection );
#endif`,rd=`#ifndef FLAT_SHADED
	varying vec3 vNormal;
	#ifdef USE_TANGENT
		varying vec3 vTangent;
		varying vec3 vBitangent;
	#endif
#endif`,sd=`#ifndef FLAT_SHADED
	varying vec3 vNormal;
	#ifdef USE_TANGENT
		varying vec3 vTangent;
		varying vec3 vBitangent;
	#endif
#endif`,ad=`#ifndef FLAT_SHADED
	vNormal = normalize( transformedNormal );
	#ifdef USE_TANGENT
		vTangent = normalize( transformedTangent );
		vBitangent = normalize( cross( vNormal, vTangent ) * tangent.w );
		#ifdef FLIP_SIDED
			vBitangent = - vBitangent;
		#endif
	#endif
#endif`,od=`#ifdef USE_NORMALMAP
	uniform sampler2D normalMap;
	uniform vec2 normalScale;
#endif
#ifdef USE_NORMALMAP_OBJECTSPACE
	uniform mat3 normalMatrix;
#endif
#if ! defined ( USE_TANGENT ) && ( defined ( USE_NORMALMAP_TANGENTSPACE ) || defined ( USE_CLEARCOAT_NORMALMAP ) || defined( USE_ANISOTROPY ) )
	mat3 getTangentFrame( vec3 eye_pos, vec3 surf_norm, vec2 uv ) {
		vec3 q0 = dFdx( eye_pos.xyz );
		vec3 q1 = dFdy( eye_pos.xyz );
		vec2 st0 = dFdx( uv.st );
		vec2 st1 = dFdy( uv.st );
		vec3 N = surf_norm;
		vec3 q1perp = cross( q1, N );
		vec3 q0perp = cross( N, q0 );
		vec3 T = q1perp * st0.x + q0perp * st1.x;
		vec3 B = q1perp * st0.y + q0perp * st1.y;
		float det = max( dot( T, T ), dot( B, B ) );
		float scale = ( det == 0.0 ) ? 0.0 : inversesqrt( det );
		return mat3( T * scale, B * scale, N );
	}
#endif`,ld=`#ifdef USE_CLEARCOAT
	vec3 clearcoatNormal = nonPerturbedNormal;
#endif`,cd=`#ifdef USE_CLEARCOAT_NORMALMAP
	vec3 clearcoatMapN = texture2D( clearcoatNormalMap, vClearcoatNormalMapUv ).xyz * 2.0 - 1.0;
	clearcoatMapN.xy *= clearcoatNormalScale;
	clearcoatNormal = normalize( tbn2 * clearcoatMapN );
#endif`,hd=`#ifdef USE_CLEARCOATMAP
	uniform sampler2D clearcoatMap;
#endif
#ifdef USE_CLEARCOAT_NORMALMAP
	uniform sampler2D clearcoatNormalMap;
	uniform vec2 clearcoatNormalScale;
#endif
#ifdef USE_CLEARCOAT_ROUGHNESSMAP
	uniform sampler2D clearcoatRoughnessMap;
#endif`,ud=`#ifdef USE_IRIDESCENCEMAP
	uniform sampler2D iridescenceMap;
#endif
#ifdef USE_IRIDESCENCE_THICKNESSMAP
	uniform sampler2D iridescenceThicknessMap;
#endif`,dd=`#ifdef OPAQUE
diffuseColor.a = 1.0;
#endif
#ifdef USE_TRANSMISSION
diffuseColor.a *= material.transmissionAlpha;
#endif
gl_FragColor = vec4( outgoingLight, diffuseColor.a );`,fd=`vec3 packNormalToRGB( const in vec3 normal ) {
	return normalize( normal ) * 0.5 + 0.5;
}
vec3 unpackRGBToNormal( const in vec3 rgb ) {
	return 2.0 * rgb.xyz - 1.0;
}
const float PackUpscale = 256. / 255.;const float UnpackDownscale = 255. / 256.;const float ShiftRight8 = 1. / 256.;
const float Inv255 = 1. / 255.;
const vec4 PackFactors = vec4( 1.0, 256.0, 256.0 * 256.0, 256.0 * 256.0 * 256.0 );
const vec2 UnpackFactors2 = vec2( UnpackDownscale, 1.0 / PackFactors.g );
const vec3 UnpackFactors3 = vec3( UnpackDownscale / PackFactors.rg, 1.0 / PackFactors.b );
const vec4 UnpackFactors4 = vec4( UnpackDownscale / PackFactors.rgb, 1.0 / PackFactors.a );
vec4 packDepthToRGBA( const in float v ) {
	if( v <= 0.0 )
		return vec4( 0., 0., 0., 0. );
	if( v >= 1.0 )
		return vec4( 1., 1., 1., 1. );
	float vuf;
	float af = modf( v * PackFactors.a, vuf );
	float bf = modf( vuf * ShiftRight8, vuf );
	float gf = modf( vuf * ShiftRight8, vuf );
	return vec4( vuf * Inv255, gf * PackUpscale, bf * PackUpscale, af );
}
vec3 packDepthToRGB( const in float v ) {
	if( v <= 0.0 )
		return vec3( 0., 0., 0. );
	if( v >= 1.0 )
		return vec3( 1., 1., 1. );
	float vuf;
	float bf = modf( v * PackFactors.b, vuf );
	float gf = modf( vuf * ShiftRight8, vuf );
	return vec3( vuf * Inv255, gf * PackUpscale, bf );
}
vec2 packDepthToRG( const in float v ) {
	if( v <= 0.0 )
		return vec2( 0., 0. );
	if( v >= 1.0 )
		return vec2( 1., 1. );
	float vuf;
	float gf = modf( v * 256., vuf );
	return vec2( vuf * Inv255, gf );
}
float unpackRGBAToDepth( const in vec4 v ) {
	return dot( v, UnpackFactors4 );
}
float unpackRGBToDepth( const in vec3 v ) {
	return dot( v, UnpackFactors3 );
}
float unpackRGToDepth( const in vec2 v ) {
	return v.r * UnpackFactors2.r + v.g * UnpackFactors2.g;
}
vec4 pack2HalfToRGBA( const in vec2 v ) {
	vec4 r = vec4( v.x, fract( v.x * 255.0 ), v.y, fract( v.y * 255.0 ) );
	return vec4( r.x - r.y / 255.0, r.y, r.z - r.w / 255.0, r.w );
}
vec2 unpackRGBATo2Half( const in vec4 v ) {
	return vec2( v.x + ( v.y / 255.0 ), v.z + ( v.w / 255.0 ) );
}
float viewZToOrthographicDepth( const in float viewZ, const in float near, const in float far ) {
	return ( viewZ + near ) / ( near - far );
}
float orthographicDepthToViewZ( const in float depth, const in float near, const in float far ) {
	#ifdef USE_REVERSED_DEPTH_BUFFER
	
		return depth * ( far - near ) - far;
	#else
		return depth * ( near - far ) - near;
	#endif
}
float viewZToPerspectiveDepth( const in float viewZ, const in float near, const in float far ) {
	return ( ( near + viewZ ) * far ) / ( ( far - near ) * viewZ );
}
float perspectiveDepthToViewZ( const in float depth, const in float near, const in float far ) {
	
	#ifdef USE_REVERSED_DEPTH_BUFFER
		return ( near * far ) / ( ( near - far ) * depth - near );
	#else
		return ( near * far ) / ( ( far - near ) * depth - far );
	#endif
}`,pd=`#ifdef PREMULTIPLIED_ALPHA
	gl_FragColor.rgb *= gl_FragColor.a;
#endif`,md=`vec4 mvPosition = vec4( transformed, 1.0 );
#ifdef USE_BATCHING
	mvPosition = batchingMatrix * mvPosition;
#endif
#ifdef USE_INSTANCING
	mvPosition = instanceMatrix * mvPosition;
#endif
mvPosition = modelViewMatrix * mvPosition;
gl_Position = projectionMatrix * mvPosition;`,_d=`#ifdef DITHERING
	gl_FragColor.rgb = dithering( gl_FragColor.rgb );
#endif`,gd=`#ifdef DITHERING
	vec3 dithering( vec3 color ) {
		float grid_position = rand( gl_FragCoord.xy );
		vec3 dither_shift_RGB = vec3( 0.25 / 255.0, -0.25 / 255.0, 0.25 / 255.0 );
		dither_shift_RGB = mix( 2.0 * dither_shift_RGB, -2.0 * dither_shift_RGB, grid_position );
		return color + dither_shift_RGB;
	}
#endif`,xd=`float roughnessFactor = roughness;
#ifdef USE_ROUGHNESSMAP
	vec4 texelRoughness = texture2D( roughnessMap, vRoughnessMapUv );
	roughnessFactor *= texelRoughness.g;
#endif`,vd=`#ifdef USE_ROUGHNESSMAP
	uniform sampler2D roughnessMap;
#endif`,Sd=`#if NUM_SPOT_LIGHT_COORDS > 0
	varying vec4 vSpotLightCoord[ NUM_SPOT_LIGHT_COORDS ];
#endif
#if NUM_SPOT_LIGHT_MAPS > 0
	uniform sampler2D spotLightMap[ NUM_SPOT_LIGHT_MAPS ];
#endif
#ifdef USE_SHADOWMAP
	#if NUM_SUN_LIGHT_SHADOWS > 0
		#define SUN_LIGHT_CASCADES 2
		#if defined( SHADOWMAP_TYPE_PCF )
			uniform sampler2DShadow sunShadowMap[ NUM_SUN_LIGHT_SHADOWS ];
		#else
			uniform sampler2D sunShadowMap[ NUM_SUN_LIGHT_SHADOWS ];
		#endif
		uniform mat4 sunShadowMatrix[ NUM_SUN_LIGHT_SHADOWS * SUN_LIGHT_CASCADES ];
		uniform vec4 sunShadowCascade[ NUM_SUN_LIGHT_SHADOWS * SUN_LIGHT_CASCADES ];
		varying vec4 vSunShadowWorldPosition;
		varying vec3 vSunShadowWorldNormal;
		struct SunLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
		};
		uniform SunLightShadow sunLightShadows[ NUM_SUN_LIGHT_SHADOWS ];
	#endif
	#if NUM_DIR_LIGHT_SHADOWS > 0
		#if defined( SHADOWMAP_TYPE_PCF )
			uniform sampler2DShadow directionalShadowMap[ NUM_DIR_LIGHT_SHADOWS ];
		#else
			uniform sampler2D directionalShadowMap[ NUM_DIR_LIGHT_SHADOWS ];
		#endif
		varying vec4 vDirectionalShadowCoord[ NUM_DIR_LIGHT_SHADOWS ];
		struct DirectionalLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
		};
		uniform DirectionalLightShadow directionalLightShadows[ NUM_DIR_LIGHT_SHADOWS ];
	#endif
	#if NUM_SPOT_LIGHT_SHADOWS > 0
		#if defined( SHADOWMAP_TYPE_PCF )
			uniform sampler2DShadow spotShadowMap[ NUM_SPOT_LIGHT_SHADOWS ];
		#else
			uniform sampler2D spotShadowMap[ NUM_SPOT_LIGHT_SHADOWS ];
		#endif
		struct SpotLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
		};
		uniform SpotLightShadow spotLightShadows[ NUM_SPOT_LIGHT_SHADOWS ];
	#endif
	#if NUM_POINT_LIGHT_SHADOWS > 0
		#if defined( SHADOWMAP_TYPE_PCF )
			uniform samplerCubeShadow pointShadowMap[ NUM_POINT_LIGHT_SHADOWS ];
		#elif defined( SHADOWMAP_TYPE_BASIC )
			uniform samplerCube pointShadowMap[ NUM_POINT_LIGHT_SHADOWS ];
		#endif
		varying vec4 vPointShadowCoord[ NUM_POINT_LIGHT_SHADOWS ];
		struct PointLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
			float shadowCameraNear;
			float shadowCameraFar;
		};
		uniform PointLightShadow pointLightShadows[ NUM_POINT_LIGHT_SHADOWS ];
	#endif
	#if defined( SHADOWMAP_TYPE_PCF )
		float interleavedGradientNoise( vec2 position ) {
			return fract( 52.9829189 * fract( dot( position, vec2( 0.06711056, 0.00583715 ) ) ) );
		}
		vec2 vogelDiskSample( int sampleIndex, int samplesCount, float phi ) {
			const float goldenAngle = 2.399963229728653;
			float r = sqrt( ( float( sampleIndex ) + 0.5 ) / float( samplesCount ) );
			float theta = float( sampleIndex ) * goldenAngle + phi;
			return vec2( cos( theta ), sin( theta ) ) * r;
		}
	#endif
	#if defined( SHADOWMAP_TYPE_PCF )
		float getShadow( sampler2DShadow shadowMap, vec2 shadowMapSize, float shadowIntensity, float shadowBias, float shadowRadius, vec4 shadowCoord ) {
			float shadow = 1.0;
			shadowCoord.xyz /= shadowCoord.w;
			shadowCoord.z += shadowBias;
			bool inFrustum = shadowCoord.x >= 0.0 && shadowCoord.x <= 1.0 && shadowCoord.y >= 0.0 && shadowCoord.y <= 1.0;
			bool frustumTest = inFrustum && shadowCoord.z <= 1.0;
			if ( frustumTest ) {
				vec2 texelSize = vec2( 1.0 ) / shadowMapSize;
				float radius = shadowRadius * texelSize.x;
				float phi = interleavedGradientNoise( gl_FragCoord.xy ) * PI2;
				shadow = (
					texture( shadowMap, vec3( shadowCoord.xy + vogelDiskSample( 0, 5, phi ) * radius, shadowCoord.z ) ) +
					texture( shadowMap, vec3( shadowCoord.xy + vogelDiskSample( 1, 5, phi ) * radius, shadowCoord.z ) ) +
					texture( shadowMap, vec3( shadowCoord.xy + vogelDiskSample( 2, 5, phi ) * radius, shadowCoord.z ) ) +
					texture( shadowMap, vec3( shadowCoord.xy + vogelDiskSample( 3, 5, phi ) * radius, shadowCoord.z ) ) +
					texture( shadowMap, vec3( shadowCoord.xy + vogelDiskSample( 4, 5, phi ) * radius, shadowCoord.z ) )
				) * 0.2;
			}
			return mix( 1.0, shadow, shadowIntensity );
		}
	#elif defined( SHADOWMAP_TYPE_VSM )
		float getShadow( sampler2D shadowMap, vec2 shadowMapSize, float shadowIntensity, float shadowBias, float shadowRadius, vec4 shadowCoord ) {
			float shadow = 1.0;
			shadowCoord.xyz /= shadowCoord.w;
			#ifdef USE_REVERSED_DEPTH_BUFFER
				shadowCoord.z -= shadowBias;
			#else
				shadowCoord.z += shadowBias;
			#endif
			bool inFrustum = shadowCoord.x >= 0.0 && shadowCoord.x <= 1.0 && shadowCoord.y >= 0.0 && shadowCoord.y <= 1.0;
			bool frustumTest = inFrustum && shadowCoord.z <= 1.0;
			if ( frustumTest ) {
				vec2 distribution = texture2D( shadowMap, shadowCoord.xy ).rg;
				float mean = distribution.x;
				float variance = distribution.y * distribution.y;
				#ifdef USE_REVERSED_DEPTH_BUFFER
					float hard_shadow = step( mean, shadowCoord.z );
				#else
					float hard_shadow = step( shadowCoord.z, mean );
				#endif
				
				if ( hard_shadow == 1.0 ) {
					shadow = 1.0;
				} else {
					variance = max( variance, 0.0000001 );
					float d = shadowCoord.z - mean;
					float p_max = variance / ( variance + d * d );
					p_max = clamp( ( p_max - 0.3 ) / 0.65, 0.0, 1.0 );
					shadow = max( hard_shadow, p_max );
				}
			}
			return mix( 1.0, shadow, shadowIntensity );
		}
	#else
		float getShadow( sampler2D shadowMap, vec2 shadowMapSize, float shadowIntensity, float shadowBias, float shadowRadius, vec4 shadowCoord ) {
			float shadow = 1.0;
			shadowCoord.xyz /= shadowCoord.w;
			#ifdef USE_REVERSED_DEPTH_BUFFER
				shadowCoord.z -= shadowBias;
			#else
				shadowCoord.z += shadowBias;
			#endif
			bool inFrustum = shadowCoord.x >= 0.0 && shadowCoord.x <= 1.0 && shadowCoord.y >= 0.0 && shadowCoord.y <= 1.0;
			bool frustumTest = inFrustum && shadowCoord.z <= 1.0;
			if ( frustumTest ) {
				float depth = texture2D( shadowMap, shadowCoord.xy ).r;
				#ifdef USE_REVERSED_DEPTH_BUFFER
					shadow = step( depth, shadowCoord.z );
				#else
					shadow = step( shadowCoord.z, depth );
				#endif
			}
			return mix( 1.0, shadow, shadowIntensity );
		}
	#endif
	#if NUM_SUN_LIGHT_SHADOWS > 0
		float getSunShadow(
			#if defined( SHADOWMAP_TYPE_PCF )
				sampler2DShadow shadowMap,
			#else
				sampler2D shadowMap,
			#endif
			SunLightShadow sunLightShadow,
			int shadowIndex
		) {
			vec4 shadowWorldPosition = vec4( vSunShadowWorldPosition.xyz + vSunShadowWorldNormal * sunLightShadow.shadowNormalBias, 1.0 );
			float viewDepth = vSunShadowWorldPosition.w;
			int cascadeOffset = shadowIndex * SUN_LIGHT_CASCADES;
			float shadow = 1.0;
			for ( int i = SUN_LIGHT_CASCADES - 1; i >= 0; i -- ) {
				vec4 cascade = sunShadowCascade[ cascadeOffset + i ];
				if ( viewDepth >= cascade.x && viewDepth < cascade.y ) {
					float cascadeShadow = getShadow(
						shadowMap,
						sunLightShadow.shadowMapSize,
						sunLightShadow.shadowIntensity,
						sunLightShadow.shadowBias,
						sunLightShadow.shadowRadius,
						sunShadowMatrix[ cascadeOffset + i ] * shadowWorldPosition
					);
					shadow = mix( cascadeShadow, shadow, smoothstep( cascade.z, cascade.y, viewDepth ) );
				}
			}
			return shadow;
		}
	#endif
	#if NUM_POINT_LIGHT_SHADOWS > 0
	#if defined( SHADOWMAP_TYPE_PCF )
	float getPointShadow( samplerCubeShadow shadowMap, vec2 shadowMapSize, float shadowIntensity, float shadowBias, float shadowRadius, vec4 shadowCoord, float shadowCameraNear, float shadowCameraFar ) {
		float shadow = 1.0;
		vec3 lightToPosition = shadowCoord.xyz;
		vec3 bd3D = normalize( lightToPosition );
		vec3 absVec = abs( lightToPosition );
		float viewSpaceZ = max( max( absVec.x, absVec.y ), absVec.z );
		if ( viewSpaceZ - shadowCameraFar <= 0.0 && viewSpaceZ - shadowCameraNear >= 0.0 ) {
			#ifdef USE_REVERSED_DEPTH_BUFFER
				float dp = ( shadowCameraNear * ( shadowCameraFar - viewSpaceZ ) ) / ( viewSpaceZ * ( shadowCameraFar - shadowCameraNear ) );
				dp -= shadowBias;
			#else
				float dp = ( shadowCameraFar * ( viewSpaceZ - shadowCameraNear ) ) / ( viewSpaceZ * ( shadowCameraFar - shadowCameraNear ) );
				dp += shadowBias;
			#endif
			float texelSize = shadowRadius / shadowMapSize.x;
			vec3 absDir = abs( bd3D );
			vec3 tangent = absDir.x > absDir.z ? vec3( 0.0, 1.0, 0.0 ) : vec3( 1.0, 0.0, 0.0 );
			tangent = normalize( cross( bd3D, tangent ) );
			vec3 bitangent = cross( bd3D, tangent );
			float phi = interleavedGradientNoise( gl_FragCoord.xy ) * PI2;
			vec2 sample0 = vogelDiskSample( 0, 5, phi );
			vec2 sample1 = vogelDiskSample( 1, 5, phi );
			vec2 sample2 = vogelDiskSample( 2, 5, phi );
			vec2 sample3 = vogelDiskSample( 3, 5, phi );
			vec2 sample4 = vogelDiskSample( 4, 5, phi );
			shadow = (
				texture( shadowMap, vec4( bd3D + ( tangent * sample0.x + bitangent * sample0.y ) * texelSize, dp ) ) +
				texture( shadowMap, vec4( bd3D + ( tangent * sample1.x + bitangent * sample1.y ) * texelSize, dp ) ) +
				texture( shadowMap, vec4( bd3D + ( tangent * sample2.x + bitangent * sample2.y ) * texelSize, dp ) ) +
				texture( shadowMap, vec4( bd3D + ( tangent * sample3.x + bitangent * sample3.y ) * texelSize, dp ) ) +
				texture( shadowMap, vec4( bd3D + ( tangent * sample4.x + bitangent * sample4.y ) * texelSize, dp ) )
			) * 0.2;
		}
		return mix( 1.0, shadow, shadowIntensity );
	}
	#elif defined( SHADOWMAP_TYPE_BASIC )
	float getPointShadow( samplerCube shadowMap, vec2 shadowMapSize, float shadowIntensity, float shadowBias, float shadowRadius, vec4 shadowCoord, float shadowCameraNear, float shadowCameraFar ) {
		float shadow = 1.0;
		vec3 lightToPosition = shadowCoord.xyz;
		vec3 absVec = abs( lightToPosition );
		float viewSpaceZ = max( max( absVec.x, absVec.y ), absVec.z );
		if ( viewSpaceZ - shadowCameraFar <= 0.0 && viewSpaceZ - shadowCameraNear >= 0.0 ) {
			float dp = ( shadowCameraFar * ( viewSpaceZ - shadowCameraNear ) ) / ( viewSpaceZ * ( shadowCameraFar - shadowCameraNear ) );
			dp += shadowBias;
			vec3 bd3D = normalize( lightToPosition );
			float depth = textureCube( shadowMap, bd3D ).r;
			#ifdef USE_REVERSED_DEPTH_BUFFER
				depth = 1.0 - depth;
			#endif
			shadow = step( dp, depth );
		}
		return mix( 1.0, shadow, shadowIntensity );
	}
	#endif
	#endif
#endif`,Md=`#if NUM_SPOT_LIGHT_COORDS > 0
	uniform mat4 spotLightMatrix[ NUM_SPOT_LIGHT_COORDS ];
	varying vec4 vSpotLightCoord[ NUM_SPOT_LIGHT_COORDS ];
#endif
#ifdef USE_SHADOWMAP
	#if NUM_SUN_LIGHT_SHADOWS > 0
		varying vec4 vSunShadowWorldPosition;
		varying vec3 vSunShadowWorldNormal;
	#endif
	#if NUM_DIR_LIGHT_SHADOWS > 0
		uniform mat4 directionalShadowMatrix[ NUM_DIR_LIGHT_SHADOWS ];
		varying vec4 vDirectionalShadowCoord[ NUM_DIR_LIGHT_SHADOWS ];
		struct DirectionalLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
		};
		uniform DirectionalLightShadow directionalLightShadows[ NUM_DIR_LIGHT_SHADOWS ];
	#endif
	#if NUM_SPOT_LIGHT_SHADOWS > 0
		struct SpotLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
		};
		uniform SpotLightShadow spotLightShadows[ NUM_SPOT_LIGHT_SHADOWS ];
	#endif
	#if NUM_POINT_LIGHT_SHADOWS > 0
		uniform mat4 pointShadowMatrix[ NUM_POINT_LIGHT_SHADOWS ];
		varying vec4 vPointShadowCoord[ NUM_POINT_LIGHT_SHADOWS ];
		struct PointLightShadow {
			float shadowIntensity;
			float shadowBias;
			float shadowNormalBias;
			float shadowRadius;
			vec2 shadowMapSize;
			float shadowCameraNear;
			float shadowCameraFar;
		};
		uniform PointLightShadow pointLightShadows[ NUM_POINT_LIGHT_SHADOWS ];
	#endif
#endif`,Ed=`#if ( defined( USE_SHADOWMAP ) && ( NUM_DIR_LIGHT_SHADOWS > 0 || NUM_SUN_LIGHT_SHADOWS > 0 || NUM_POINT_LIGHT_SHADOWS > 0 ) ) || ( NUM_SPOT_LIGHT_COORDS > 0 )
	#ifdef HAS_NORMAL
		vec3 shadowWorldNormal = transformNormalByInverseViewMatrix( transformedNormal, viewMatrix );
	#else
		vec3 shadowWorldNormal = vec3( 0.0 );
	#endif
	vec4 shadowWorldPosition;
#endif
#if defined( USE_SHADOWMAP )
	#if NUM_SUN_LIGHT_SHADOWS > 0
		vSunShadowWorldPosition = vec4( worldPosition.xyz, - mvPosition.z );
		vSunShadowWorldNormal = shadowWorldNormal;
	#endif
	#if NUM_DIR_LIGHT_SHADOWS > 0
		#pragma unroll_loop_start
		for ( int i = 0; i < NUM_DIR_LIGHT_SHADOWS; i ++ ) {
			shadowWorldPosition = worldPosition + vec4( shadowWorldNormal * directionalLightShadows[ i ].shadowNormalBias, 0 );
			vDirectionalShadowCoord[ i ] = directionalShadowMatrix[ i ] * shadowWorldPosition;
		}
		#pragma unroll_loop_end
	#endif
	#if NUM_POINT_LIGHT_SHADOWS > 0
		#pragma unroll_loop_start
		for ( int i = 0; i < NUM_POINT_LIGHT_SHADOWS; i ++ ) {
			shadowWorldPosition = worldPosition + vec4( shadowWorldNormal * pointLightShadows[ i ].shadowNormalBias, 0 );
			vPointShadowCoord[ i ] = pointShadowMatrix[ i ] * shadowWorldPosition;
		}
		#pragma unroll_loop_end
	#endif
#endif
#if NUM_SPOT_LIGHT_COORDS > 0
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_SPOT_LIGHT_COORDS; i ++ ) {
		shadowWorldPosition = worldPosition;
		#if ( defined( USE_SHADOWMAP ) && UNROLLED_LOOP_INDEX < NUM_SPOT_LIGHT_SHADOWS )
			shadowWorldPosition.xyz += shadowWorldNormal * spotLightShadows[ i ].shadowNormalBias;
		#endif
		vSpotLightCoord[ i ] = spotLightMatrix[ i ] * shadowWorldPosition;
	}
	#pragma unroll_loop_end
#endif`,yd=`float getShadowMask() {
	float shadow = 1.0;
	#ifdef USE_SHADOWMAP
	#if NUM_SUN_LIGHT_SHADOWS > 0
	SunLightShadow sunLight;
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_SUN_LIGHT_SHADOWS; i ++ ) {
		sunLight = sunLightShadows[ i ];
		shadow *= receiveShadow ? getSunShadow( sunShadowMap[ i ], sunLight, UNROLLED_LOOP_INDEX ) : 1.0;
	}
	#pragma unroll_loop_end
	#endif
	#if NUM_DIR_LIGHT_SHADOWS > 0
	DirectionalLightShadow directionalLight;
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_DIR_LIGHT_SHADOWS; i ++ ) {
		directionalLight = directionalLightShadows[ i ];
		shadow *= receiveShadow ? getShadow( directionalShadowMap[ i ], directionalLight.shadowMapSize, directionalLight.shadowIntensity, directionalLight.shadowBias, directionalLight.shadowRadius, vDirectionalShadowCoord[ i ] ) : 1.0;
	}
	#pragma unroll_loop_end
	#endif
	#if NUM_SPOT_LIGHT_SHADOWS > 0
	SpotLightShadow spotLight;
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_SPOT_LIGHT_SHADOWS; i ++ ) {
		spotLight = spotLightShadows[ i ];
		shadow *= receiveShadow ? getShadow( spotShadowMap[ i ], spotLight.shadowMapSize, spotLight.shadowIntensity, spotLight.shadowBias, spotLight.shadowRadius, vSpotLightCoord[ i ] ) : 1.0;
	}
	#pragma unroll_loop_end
	#endif
	#if NUM_POINT_LIGHT_SHADOWS > 0 && ( defined( SHADOWMAP_TYPE_PCF ) || defined( SHADOWMAP_TYPE_BASIC ) )
	PointLightShadow pointLight;
	#pragma unroll_loop_start
	for ( int i = 0; i < NUM_POINT_LIGHT_SHADOWS; i ++ ) {
		pointLight = pointLightShadows[ i ];
		shadow *= receiveShadow ? getPointShadow( pointShadowMap[ i ], pointLight.shadowMapSize, pointLight.shadowIntensity, pointLight.shadowBias, pointLight.shadowRadius, vPointShadowCoord[ i ], pointLight.shadowCameraNear, pointLight.shadowCameraFar ) : 1.0;
	}
	#pragma unroll_loop_end
	#endif
	#endif
	return shadow;
}`,bd=`#ifdef USE_SKINNING
	mat4 boneMatX = getBoneMatrix( skinIndex.x );
	mat4 boneMatY = getBoneMatrix( skinIndex.y );
	mat4 boneMatZ = getBoneMatrix( skinIndex.z );
	mat4 boneMatW = getBoneMatrix( skinIndex.w );
#endif`,Td=`#ifdef USE_SKINNING
	uniform mat4 bindMatrix;
	uniform mat4 bindMatrixInverse;
	uniform highp sampler2D boneTexture;
	mat4 getBoneMatrix( const in float i ) {
		int size = textureSize( boneTexture, 0 ).x;
		int j = int( i ) * 4;
		int x = j % size;
		int y = j / size;
		vec4 v1 = texelFetch( boneTexture, ivec2( x, y ), 0 );
		vec4 v2 = texelFetch( boneTexture, ivec2( x + 1, y ), 0 );
		vec4 v3 = texelFetch( boneTexture, ivec2( x + 2, y ), 0 );
		vec4 v4 = texelFetch( boneTexture, ivec2( x + 3, y ), 0 );
		return mat4( v1, v2, v3, v4 );
	}
#endif`,Ad=`#ifdef USE_SKINNING
	vec4 skinVertex = bindMatrix * vec4( transformed, 1.0 );
	vec4 skinned = vec4( 0.0 );
	skinned += boneMatX * skinVertex * skinWeight.x;
	skinned += boneMatY * skinVertex * skinWeight.y;
	skinned += boneMatZ * skinVertex * skinWeight.z;
	skinned += boneMatW * skinVertex * skinWeight.w;
	transformed = ( bindMatrixInverse * skinned ).xyz;
#endif`,wd=`#ifdef USE_SKINNING
	mat4 skinMatrix = mat4( 0.0 );
	skinMatrix += skinWeight.x * boneMatX;
	skinMatrix += skinWeight.y * boneMatY;
	skinMatrix += skinWeight.z * boneMatZ;
	skinMatrix += skinWeight.w * boneMatW;
	skinMatrix = bindMatrixInverse * skinMatrix * bindMatrix;
	objectNormal = vec4( skinMatrix * vec4( objectNormal, 0.0 ) ).xyz;
	#ifdef USE_TANGENT
		objectTangent = vec4( skinMatrix * vec4( objectTangent, 0.0 ) ).xyz;
	#endif
#endif`,Rd=`float specularStrength;
#ifdef USE_SPECULARMAP
	vec4 texelSpecular = texture2D( specularMap, vSpecularMapUv );
	specularStrength = texelSpecular.r;
#else
	specularStrength = 1.0;
#endif`,Cd=`#ifdef USE_SPECULARMAP
	uniform sampler2D specularMap;
#endif`,Pd=`#if defined( TONE_MAPPING )
	gl_FragColor.rgb = toneMapping( gl_FragColor.rgb );
#endif`,Dd=`#ifndef saturate
#define saturate( a ) clamp( a, 0.0, 1.0 )
#endif
uniform float toneMappingExposure;
vec3 LinearToneMapping( vec3 color ) {
	return saturate( toneMappingExposure * color );
}
vec3 ReinhardToneMapping( vec3 color ) {
	color *= toneMappingExposure;
	return saturate( color / ( vec3( 1.0 ) + color ) );
}
vec3 CineonToneMapping( vec3 color ) {
	color *= toneMappingExposure;
	color = max( vec3( 0.0 ), color - 0.004 );
	return pow( ( color * ( 6.2 * color + 0.5 ) ) / ( color * ( 6.2 * color + 1.7 ) + 0.06 ), vec3( 2.2 ) );
}
vec3 RRTAndODTFit( vec3 v ) {
	vec3 a = v * ( v + 0.0245786 ) - 0.000090537;
	vec3 b = v * ( 0.983729 * v + 0.4329510 ) + 0.238081;
	return a / b;
}
vec3 ACESFilmicToneMapping( vec3 color ) {
	const mat3 ACESInputMat = mat3(
		vec3( 0.59719, 0.07600, 0.02840 ),		vec3( 0.35458, 0.90834, 0.13383 ),
		vec3( 0.04823, 0.01566, 0.83777 )
	);
	const mat3 ACESOutputMat = mat3(
		vec3(  1.60475, -0.10208, -0.00327 ),		vec3( -0.53108,  1.10813, -0.07276 ),
		vec3( -0.07367, -0.00605,  1.07602 )
	);
	color *= toneMappingExposure / 0.6;
	color = ACESInputMat * color;
	color = RRTAndODTFit( color );
	color = ACESOutputMat * color;
	return saturate( color );
}
const mat3 LINEAR_REC2020_TO_LINEAR_SRGB = mat3(
	vec3( 1.6605, - 0.1246, - 0.0182 ),
	vec3( - 0.5876, 1.1329, - 0.1006 ),
	vec3( - 0.0728, - 0.0083, 1.1187 )
);
const mat3 LINEAR_SRGB_TO_LINEAR_REC2020 = mat3(
	vec3( 0.6274, 0.0691, 0.0164 ),
	vec3( 0.3293, 0.9195, 0.0880 ),
	vec3( 0.0433, 0.0113, 0.8956 )
);
vec3 agxDefaultContrastApprox( vec3 x ) {
	vec3 x2 = x * x;
	vec3 x4 = x2 * x2;
	return + 15.5 * x4 * x2
		- 40.14 * x4 * x
		+ 31.96 * x4
		- 6.868 * x2 * x
		+ 0.4298 * x2
		+ 0.1191 * x
		- 0.00232;
}
vec3 AgXToneMapping( vec3 color ) {
	const mat3 AgXInsetMatrix = mat3(
		vec3( 0.856627153315983, 0.137318972929847, 0.11189821299995 ),
		vec3( 0.0951212405381588, 0.761241990602591, 0.0767994186031903 ),
		vec3( 0.0482516061458583, 0.101439036467562, 0.811302368396859 )
	);
	const mat3 AgXOutsetMatrix = mat3(
		vec3( 1.1271005818144368, - 0.1413297634984383, - 0.14132976349843826 ),
		vec3( - 0.11060664309660323, 1.157823702216272, - 0.11060664309660294 ),
		vec3( - 0.016493938717834573, - 0.016493938717834257, 1.2519364065950405 )
	);
	const float AgxMinEv = - 12.47393;	const float AgxMaxEv = 4.026069;
	color *= toneMappingExposure;
	color = LINEAR_SRGB_TO_LINEAR_REC2020 * color;
	color = AgXInsetMatrix * color;
	color = max( color, 1e-10 );	color = log2( color );
	color = ( color - AgxMinEv ) / ( AgxMaxEv - AgxMinEv );
	color = clamp( color, 0.0, 1.0 );
	color = agxDefaultContrastApprox( color );
	color = AgXOutsetMatrix * color;
	color = pow( max( vec3( 0.0 ), color ), vec3( 2.2 ) );
	color = LINEAR_REC2020_TO_LINEAR_SRGB * color;
	color = clamp( color, 0.0, 1.0 );
	return color;
}
vec3 NeutralToneMapping( vec3 color ) {
	const float StartCompression = 0.8 - 0.04;
	const float Desaturation = 0.15;
	color *= toneMappingExposure;
	float x = min( color.r, min( color.g, color.b ) );
	float offset = x < 0.08 ? x - 6.25 * x * x : 0.04;
	color -= offset;
	float peak = max( color.r, max( color.g, color.b ) );
	if ( peak < StartCompression ) return color;
	float d = 1. - StartCompression;
	float newPeak = 1. - d * d / ( peak + d - StartCompression );
	color *= newPeak / peak;
	float g = 1. - 1. / ( Desaturation * ( peak - newPeak ) + 1. );
	return mix( color, vec3( newPeak ), g );
}
vec3 CustomToneMapping( vec3 color ) { return color; }`,Ld=`#ifdef USE_TRANSMISSION
	material.transmission = transmission;
	material.transmissionAlpha = 1.0;
	material.thickness = thickness;
	material.attenuationDistance = attenuationDistance;
	material.attenuationColor = attenuationColor;
	#ifdef USE_TRANSMISSIONMAP
		material.transmission *= texture2D( transmissionMap, vTransmissionMapUv ).r;
	#endif
	#ifdef USE_THICKNESSMAP
		material.thickness *= texture2D( thicknessMap, vThicknessMapUv ).g;
	#endif
	vec3 pos = vWorldPosition;
	vec3 v = normalize( cameraPosition - pos );
	vec3 n = transformNormalByInverseViewMatrix( normal, viewMatrix );
	vec4 transmitted = getIBLVolumeRefraction(
		n, v, material.roughness, material.diffuseContribution, material.specularColorBlended, material.specularF90,
		pos, modelMatrix, viewMatrix, projectionMatrix, material.dispersion, material.ior, material.thickness,
		material.attenuationColor, material.attenuationDistance );
	material.transmissionAlpha = mix( material.transmissionAlpha, transmitted.a, material.transmission );
	totalDiffuse = mix( totalDiffuse, transmitted.rgb, material.transmission );
#endif`,Id=`#ifdef USE_TRANSMISSION
	uniform float transmission;
	uniform float thickness;
	uniform float attenuationDistance;
	uniform vec3 attenuationColor;
	#ifdef USE_TRANSMISSIONMAP
		uniform sampler2D transmissionMap;
	#endif
	#ifdef USE_THICKNESSMAP
		uniform sampler2D thicknessMap;
	#endif
	uniform vec2 transmissionSamplerSize;
	uniform sampler2D transmissionSamplerMap;
	uniform mat4 modelMatrix;
	uniform mat4 projectionMatrix;
	varying vec3 vWorldPosition;
	float w0( float a ) {
		return ( 1.0 / 6.0 ) * ( a * ( a * ( - a + 3.0 ) - 3.0 ) + 1.0 );
	}
	float w1( float a ) {
		return ( 1.0 / 6.0 ) * ( a *  a * ( 3.0 * a - 6.0 ) + 4.0 );
	}
	float w2( float a ){
		return ( 1.0 / 6.0 ) * ( a * ( a * ( - 3.0 * a + 3.0 ) + 3.0 ) + 1.0 );
	}
	float w3( float a ) {
		return ( 1.0 / 6.0 ) * ( a * a * a );
	}
	float g0( float a ) {
		return w0( a ) + w1( a );
	}
	float g1( float a ) {
		return w2( a ) + w3( a );
	}
	float h0( float a ) {
		return - 1.0 + w1( a ) / ( w0( a ) + w1( a ) );
	}
	float h1( float a ) {
		return 1.0 + w3( a ) / ( w2( a ) + w3( a ) );
	}
	vec4 bicubic( sampler2D tex, vec2 uv, vec4 texelSize, float lod ) {
		uv = uv * texelSize.zw + 0.5;
		vec2 iuv = floor( uv );
		vec2 fuv = fract( uv );
		float g0x = g0( fuv.x );
		float g1x = g1( fuv.x );
		float h0x = h0( fuv.x );
		float h1x = h1( fuv.x );
		float h0y = h0( fuv.y );
		float h1y = h1( fuv.y );
		vec2 p0 = ( vec2( iuv.x + h0x, iuv.y + h0y ) - 0.5 ) * texelSize.xy;
		vec2 p1 = ( vec2( iuv.x + h1x, iuv.y + h0y ) - 0.5 ) * texelSize.xy;
		vec2 p2 = ( vec2( iuv.x + h0x, iuv.y + h1y ) - 0.5 ) * texelSize.xy;
		vec2 p3 = ( vec2( iuv.x + h1x, iuv.y + h1y ) - 0.5 ) * texelSize.xy;
		return g0( fuv.y ) * ( g0x * textureLod( tex, p0, lod ) + g1x * textureLod( tex, p1, lod ) ) +
			g1( fuv.y ) * ( g0x * textureLod( tex, p2, lod ) + g1x * textureLod( tex, p3, lod ) );
	}
	vec4 textureBicubic( sampler2D sampler, vec2 uv, float lod ) {
		vec2 fLodSize = vec2( textureSize( sampler, int( lod ) ) );
		vec2 cLodSize = vec2( textureSize( sampler, int( lod + 1.0 ) ) );
		vec2 fLodSizeInv = 1.0 / fLodSize;
		vec2 cLodSizeInv = 1.0 / cLodSize;
		vec4 fSample = bicubic( sampler, uv, vec4( fLodSizeInv, fLodSize ), floor( lod ) );
		vec4 cSample = bicubic( sampler, uv, vec4( cLodSizeInv, cLodSize ), ceil( lod ) );
		return mix( fSample, cSample, fract( lod ) );
	}
	vec3 getVolumeTransmissionRay( const in vec3 n, const in vec3 v, const in float thickness, const in float ior, const in mat4 modelMatrix ) {
		vec3 refractionVector = refract( - v, normalize( n ), 1.0 / ior );
		vec3 modelScale;
		modelScale.x = length( vec3( modelMatrix[ 0 ].xyz ) );
		modelScale.y = length( vec3( modelMatrix[ 1 ].xyz ) );
		modelScale.z = length( vec3( modelMatrix[ 2 ].xyz ) );
		return normalize( refractionVector ) * thickness * modelScale;
	}
	float applyIorToRoughness( const in float roughness, const in float ior ) {
		return roughness * clamp( ior * 2.0 - 2.0, 0.0, 1.0 );
	}
	vec4 getTransmissionSample( const in vec2 fragCoord, const in float roughness, const in float ior ) {
		float lod = log2( transmissionSamplerSize.x ) * applyIorToRoughness( roughness, ior );
		return textureBicubic( transmissionSamplerMap, fragCoord.xy, lod );
	}
	vec3 volumeAttenuation( const in float transmissionDistance, const in vec3 attenuationColor, const in float attenuationDistance ) {
		if ( isinf( attenuationDistance ) ) {
			return vec3( 1.0 );
		} else {
			vec3 attenuationCoefficient = -log( attenuationColor ) / attenuationDistance;
			vec3 transmittance = exp( - attenuationCoefficient * transmissionDistance );			return transmittance;
		}
	}
	vec4 getIBLVolumeRefraction( const in vec3 n, const in vec3 v, const in float roughness, const in vec3 diffuseColor,
		const in vec3 specularColor, const in float specularF90, const in vec3 position, const in mat4 modelMatrix,
		const in mat4 viewMatrix, const in mat4 projMatrix, const in float dispersion, const in float ior, const in float thickness,
		const in vec3 attenuationColor, const in float attenuationDistance ) {
		vec4 transmittedLight;
		vec3 transmittance;
		#ifdef USE_DISPERSION
			float halfSpread = ( ior - 1.0 ) * 0.025 * dispersion;
			vec3 iors = vec3( ior - halfSpread, ior, ior + halfSpread );
			for ( int i = 0; i < 3; i ++ ) {
				vec3 transmissionRay = getVolumeTransmissionRay( n, v, thickness, iors[ i ], modelMatrix );
				vec3 refractedRayExit = position + transmissionRay;
				vec4 ndcPos = projMatrix * viewMatrix * vec4( refractedRayExit, 1.0 );
				vec2 refractionCoords = ndcPos.xy / ndcPos.w;
				refractionCoords += 1.0;
				refractionCoords /= 2.0;
				vec4 transmissionSample = getTransmissionSample( refractionCoords, roughness, iors[ i ] );
				transmittedLight[ i ] = transmissionSample[ i ];
				transmittedLight.a += transmissionSample.a;
				transmittance[ i ] = diffuseColor[ i ] * volumeAttenuation( length( transmissionRay ), attenuationColor, attenuationDistance )[ i ];
			}
			transmittedLight.a /= 3.0;
		#else
			vec3 transmissionRay = getVolumeTransmissionRay( n, v, thickness, ior, modelMatrix );
			vec3 refractedRayExit = position + transmissionRay;
			vec4 ndcPos = projMatrix * viewMatrix * vec4( refractedRayExit, 1.0 );
			vec2 refractionCoords = ndcPos.xy / ndcPos.w;
			refractionCoords += 1.0;
			refractionCoords /= 2.0;
			transmittedLight = getTransmissionSample( refractionCoords, roughness, ior );
			transmittance = diffuseColor * volumeAttenuation( length( transmissionRay ), attenuationColor, attenuationDistance );
		#endif
		vec3 attenuatedColor = transmittance * transmittedLight.rgb;
		vec3 F = EnvironmentBRDF( n, v, specularColor, specularF90, roughness );
		float transmittanceFactor = ( transmittance.r + transmittance.g + transmittance.b ) / 3.0;
		return vec4( ( 1.0 - F ) * attenuatedColor, 1.0 - ( 1.0 - transmittedLight.a ) * transmittanceFactor );
	}
#endif`,Ud=`#if defined( USE_UV ) || defined( USE_ANISOTROPY )
	varying vec2 vUv;
#endif
#ifdef USE_MAP
	varying vec2 vMapUv;
#endif
#ifdef USE_ALPHAMAP
	varying vec2 vAlphaMapUv;
#endif
#ifdef USE_LIGHTMAP
	varying vec2 vLightMapUv;
#endif
#ifdef USE_AOMAP
	varying vec2 vAoMapUv;
#endif
#ifdef USE_BUMPMAP
	varying vec2 vBumpMapUv;
#endif
#ifdef USE_NORMALMAP
	varying vec2 vNormalMapUv;
#endif
#ifdef USE_EMISSIVEMAP
	varying vec2 vEmissiveMapUv;
#endif
#ifdef USE_METALNESSMAP
	varying vec2 vMetalnessMapUv;
#endif
#ifdef USE_ROUGHNESSMAP
	varying vec2 vRoughnessMapUv;
#endif
#ifdef USE_ANISOTROPYMAP
	varying vec2 vAnisotropyMapUv;
#endif
#ifdef USE_CLEARCOATMAP
	varying vec2 vClearcoatMapUv;
#endif
#ifdef USE_CLEARCOAT_NORMALMAP
	varying vec2 vClearcoatNormalMapUv;
#endif
#ifdef USE_CLEARCOAT_ROUGHNESSMAP
	varying vec2 vClearcoatRoughnessMapUv;
#endif
#ifdef USE_IRIDESCENCEMAP
	varying vec2 vIridescenceMapUv;
#endif
#ifdef USE_IRIDESCENCE_THICKNESSMAP
	varying vec2 vIridescenceThicknessMapUv;
#endif
#ifdef USE_SHEEN_COLORMAP
	varying vec2 vSheenColorMapUv;
#endif
#ifdef USE_SHEEN_ROUGHNESSMAP
	varying vec2 vSheenRoughnessMapUv;
#endif
#ifdef USE_SPECULARMAP
	varying vec2 vSpecularMapUv;
#endif
#ifdef USE_SPECULAR_COLORMAP
	varying vec2 vSpecularColorMapUv;
#endif
#ifdef USE_SPECULAR_INTENSITYMAP
	varying vec2 vSpecularIntensityMapUv;
#endif
#ifdef USE_TRANSMISSIONMAP
	uniform mat3 transmissionMapTransform;
	varying vec2 vTransmissionMapUv;
#endif
#ifdef USE_THICKNESSMAP
	uniform mat3 thicknessMapTransform;
	varying vec2 vThicknessMapUv;
#endif`,Nd=`#if defined( USE_UV ) || defined( USE_ANISOTROPY )
	varying vec2 vUv;
#endif
#ifdef USE_MAP
	uniform mat3 mapTransform;
	varying vec2 vMapUv;
#endif
#ifdef USE_ALPHAMAP
	uniform mat3 alphaMapTransform;
	varying vec2 vAlphaMapUv;
#endif
#ifdef USE_LIGHTMAP
	uniform mat3 lightMapTransform;
	varying vec2 vLightMapUv;
#endif
#ifdef USE_AOMAP
	uniform mat3 aoMapTransform;
	varying vec2 vAoMapUv;
#endif
#ifdef USE_BUMPMAP
	uniform mat3 bumpMapTransform;
	varying vec2 vBumpMapUv;
#endif
#ifdef USE_NORMALMAP
	uniform mat3 normalMapTransform;
	varying vec2 vNormalMapUv;
#endif
#ifdef USE_DISPLACEMENTMAP
	uniform mat3 displacementMapTransform;
	varying vec2 vDisplacementMapUv;
#endif
#ifdef USE_EMISSIVEMAP
	uniform mat3 emissiveMapTransform;
	varying vec2 vEmissiveMapUv;
#endif
#ifdef USE_METALNESSMAP
	uniform mat3 metalnessMapTransform;
	varying vec2 vMetalnessMapUv;
#endif
#ifdef USE_ROUGHNESSMAP
	uniform mat3 roughnessMapTransform;
	varying vec2 vRoughnessMapUv;
#endif
#ifdef USE_ANISOTROPYMAP
	uniform mat3 anisotropyMapTransform;
	varying vec2 vAnisotropyMapUv;
#endif
#ifdef USE_CLEARCOATMAP
	uniform mat3 clearcoatMapTransform;
	varying vec2 vClearcoatMapUv;
#endif
#ifdef USE_CLEARCOAT_NORMALMAP
	uniform mat3 clearcoatNormalMapTransform;
	varying vec2 vClearcoatNormalMapUv;
#endif
#ifdef USE_CLEARCOAT_ROUGHNESSMAP
	uniform mat3 clearcoatRoughnessMapTransform;
	varying vec2 vClearcoatRoughnessMapUv;
#endif
#ifdef USE_SHEEN_COLORMAP
	uniform mat3 sheenColorMapTransform;
	varying vec2 vSheenColorMapUv;
#endif
#ifdef USE_SHEEN_ROUGHNESSMAP
	uniform mat3 sheenRoughnessMapTransform;
	varying vec2 vSheenRoughnessMapUv;
#endif
#ifdef USE_IRIDESCENCEMAP
	uniform mat3 iridescenceMapTransform;
	varying vec2 vIridescenceMapUv;
#endif
#ifdef USE_IRIDESCENCE_THICKNESSMAP
	uniform mat3 iridescenceThicknessMapTransform;
	varying vec2 vIridescenceThicknessMapUv;
#endif
#ifdef USE_SPECULARMAP
	uniform mat3 specularMapTransform;
	varying vec2 vSpecularMapUv;
#endif
#ifdef USE_SPECULAR_COLORMAP
	uniform mat3 specularColorMapTransform;
	varying vec2 vSpecularColorMapUv;
#endif
#ifdef USE_SPECULAR_INTENSITYMAP
	uniform mat3 specularIntensityMapTransform;
	varying vec2 vSpecularIntensityMapUv;
#endif
#ifdef USE_TRANSMISSIONMAP
	uniform mat3 transmissionMapTransform;
	varying vec2 vTransmissionMapUv;
#endif
#ifdef USE_THICKNESSMAP
	uniform mat3 thicknessMapTransform;
	varying vec2 vThicknessMapUv;
#endif`,Fd=`#if defined( USE_UV ) || defined( USE_ANISOTROPY )
	vUv = vec3( uv, 1 ).xy;
#endif
#ifdef USE_MAP
	vMapUv = ( mapTransform * vec3( MAP_UV, 1 ) ).xy;
#endif
#ifdef USE_ALPHAMAP
	vAlphaMapUv = ( alphaMapTransform * vec3( ALPHAMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_LIGHTMAP
	vLightMapUv = ( lightMapTransform * vec3( LIGHTMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_AOMAP
	vAoMapUv = ( aoMapTransform * vec3( AOMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_BUMPMAP
	vBumpMapUv = ( bumpMapTransform * vec3( BUMPMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_NORMALMAP
	vNormalMapUv = ( normalMapTransform * vec3( NORMALMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_DISPLACEMENTMAP
	vDisplacementMapUv = ( displacementMapTransform * vec3( DISPLACEMENTMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_EMISSIVEMAP
	vEmissiveMapUv = ( emissiveMapTransform * vec3( EMISSIVEMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_METALNESSMAP
	vMetalnessMapUv = ( metalnessMapTransform * vec3( METALNESSMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_ROUGHNESSMAP
	vRoughnessMapUv = ( roughnessMapTransform * vec3( ROUGHNESSMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_ANISOTROPYMAP
	vAnisotropyMapUv = ( anisotropyMapTransform * vec3( ANISOTROPYMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_CLEARCOATMAP
	vClearcoatMapUv = ( clearcoatMapTransform * vec3( CLEARCOATMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_CLEARCOAT_NORMALMAP
	vClearcoatNormalMapUv = ( clearcoatNormalMapTransform * vec3( CLEARCOAT_NORMALMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_CLEARCOAT_ROUGHNESSMAP
	vClearcoatRoughnessMapUv = ( clearcoatRoughnessMapTransform * vec3( CLEARCOAT_ROUGHNESSMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_IRIDESCENCEMAP
	vIridescenceMapUv = ( iridescenceMapTransform * vec3( IRIDESCENCEMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_IRIDESCENCE_THICKNESSMAP
	vIridescenceThicknessMapUv = ( iridescenceThicknessMapTransform * vec3( IRIDESCENCE_THICKNESSMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_SHEEN_COLORMAP
	vSheenColorMapUv = ( sheenColorMapTransform * vec3( SHEEN_COLORMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_SHEEN_ROUGHNESSMAP
	vSheenRoughnessMapUv = ( sheenRoughnessMapTransform * vec3( SHEEN_ROUGHNESSMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_SPECULARMAP
	vSpecularMapUv = ( specularMapTransform * vec3( SPECULARMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_SPECULAR_COLORMAP
	vSpecularColorMapUv = ( specularColorMapTransform * vec3( SPECULAR_COLORMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_SPECULAR_INTENSITYMAP
	vSpecularIntensityMapUv = ( specularIntensityMapTransform * vec3( SPECULAR_INTENSITYMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_TRANSMISSIONMAP
	vTransmissionMapUv = ( transmissionMapTransform * vec3( TRANSMISSIONMAP_UV, 1 ) ).xy;
#endif
#ifdef USE_THICKNESSMAP
	vThicknessMapUv = ( thicknessMapTransform * vec3( THICKNESSMAP_UV, 1 ) ).xy;
#endif`,Od=`#if defined( USE_ENVMAP ) || defined( DISTANCE ) || defined ( USE_SHADOWMAP ) || defined ( USE_TRANSMISSION ) || NUM_SPOT_LIGHT_COORDS > 0
	vec4 worldPosition = vec4( transformed, 1.0 );
	#ifdef USE_BATCHING
		worldPosition = batchingMatrix * worldPosition;
	#endif
	#ifdef USE_INSTANCING
		worldPosition = instanceMatrix * worldPosition;
	#endif
	worldPosition = modelMatrix * worldPosition;
#endif`;const Bd=`varying vec2 vUv;
uniform mat3 uvTransform;
void main() {
	vUv = ( uvTransform * vec3( uv, 1 ) ).xy;
	gl_Position = vec4( position.xy, 1.0, 1.0 );
}`,zd=`uniform sampler2D t2D;
uniform float backgroundIntensity;
varying vec2 vUv;
void main() {
	vec4 texColor = texture2D( t2D, vUv );
	#ifdef DECODE_VIDEO_TEXTURE
		texColor = vec4( mix( pow( texColor.rgb * 0.9478672986 + vec3( 0.0521327014 ), vec3( 2.4 ) ), texColor.rgb * 0.0773993808, vec3( lessThanEqual( texColor.rgb, vec3( 0.04045 ) ) ) ), texColor.w );
	#endif
	texColor.rgb *= backgroundIntensity;
	gl_FragColor = texColor;
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
}`,Gd=`varying vec3 vWorldDirection;
#include <common>
void main() {
	vWorldDirection = transformDirection( position, modelMatrix );
	#include <begin_vertex>
	#include <project_vertex>
	gl_Position.z = gl_Position.w;
}`,Hd=`#ifdef ENVMAP_TYPE_CUBE
	uniform samplerCube envMap;
#elif defined( ENVMAP_TYPE_CUBE_UV )
	uniform sampler2D envMap;
#endif
uniform float backgroundBlurriness;
uniform float backgroundIntensity;
uniform mat3 backgroundRotation;
varying vec3 vWorldDirection;
#include <cube_uv_reflection_fragment>
void main() {
	#ifdef ENVMAP_TYPE_CUBE
		vec4 texColor = textureCube( envMap, backgroundRotation * vWorldDirection );
	#elif defined( ENVMAP_TYPE_CUBE_UV )
		vec4 texColor = textureCubeUV( envMap, backgroundRotation * vWorldDirection, backgroundBlurriness );
	#else
		vec4 texColor = vec4( 0.0, 0.0, 0.0, 1.0 );
	#endif
	texColor.rgb *= backgroundIntensity;
	gl_FragColor = texColor;
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
}`,Vd=`varying vec3 vWorldDirection;
#include <common>
void main() {
	vWorldDirection = transformDirection( position, modelMatrix );
	#include <begin_vertex>
	#include <project_vertex>
	gl_Position.z = gl_Position.w;
}`,kd=`uniform samplerCube tCube;
uniform float tFlip;
uniform float opacity;
varying vec3 vWorldDirection;
void main() {
	vec4 texColor = textureCube( tCube, vec3( tFlip * vWorldDirection.x, vWorldDirection.yz ) );
	gl_FragColor = texColor;
	gl_FragColor.a *= opacity;
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
}`,Wd=`#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
varying vec2 vHighPrecisionZW;
void main() {
	#include <uv_vertex>
	#include <batching_vertex>
	#include <skinbase_vertex>
	#include <morphinstance_vertex>
	#ifdef USE_DISPLACEMENTMAP
		#include <beginnormal_vertex>
		#include <morphnormal_vertex>
		#include <skinnormal_vertex>
	#endif
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	vHighPrecisionZW = gl_Position.zw;
}`,Xd=`#if DEPTH_PACKING == 3200
	uniform float opacity;
#endif
#include <common>
#include <packing>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
varying vec2 vHighPrecisionZW;
void main() {
	vec4 diffuseColor = vec4( 1.0 );
	#include <clipping_planes_fragment>
	#if DEPTH_PACKING == 3200
		diffuseColor.a = opacity;
	#endif
	#include <map_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <logdepthbuf_fragment>
	#ifdef USE_REVERSED_DEPTH_BUFFER
		float fragCoordZ = vHighPrecisionZW[ 0 ] / vHighPrecisionZW[ 1 ];
	#else
		float fragCoordZ = 0.5 * vHighPrecisionZW[ 0 ] / vHighPrecisionZW[ 1 ] + 0.5;
	#endif
	#if DEPTH_PACKING == 3200
		gl_FragColor = vec4( vec3( 1.0 - fragCoordZ ), opacity );
	#elif DEPTH_PACKING == 3201
		gl_FragColor = packDepthToRGBA( fragCoordZ );
	#elif DEPTH_PACKING == 3202
		gl_FragColor = vec4( packDepthToRGB( fragCoordZ ), 1.0 );
	#elif DEPTH_PACKING == 3203
		gl_FragColor = vec4( packDepthToRG( fragCoordZ ), 0.0, 1.0 );
	#endif
}`,Yd=`#define DISTANCE
varying vec3 vWorldPosition;
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <batching_vertex>
	#include <skinbase_vertex>
	#include <morphinstance_vertex>
	#ifdef USE_DISPLACEMENTMAP
		#include <beginnormal_vertex>
		#include <morphnormal_vertex>
		#include <skinnormal_vertex>
	#endif
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <worldpos_vertex>
	#include <clipping_planes_vertex>
	vWorldPosition = worldPosition.xyz;
}`,qd=`#define DISTANCE
uniform vec3 referencePosition;
uniform float nearDistance;
uniform float farDistance;
varying vec3 vWorldPosition;
#include <common>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( 1.0 );
	#include <clipping_planes_fragment>
	#include <map_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	float dist = length( vWorldPosition - referencePosition );
	dist = ( dist - nearDistance ) / ( farDistance - nearDistance );
	dist = saturate( dist );
	gl_FragColor = vec4( dist, 0.0, 0.0, 1.0 );
}`,Zd=`varying vec3 vWorldDirection;
#include <common>
void main() {
	vWorldDirection = transformDirection( position, modelMatrix );
	#include <begin_vertex>
	#include <project_vertex>
}`,Kd=`uniform sampler2D tEquirect;
varying vec3 vWorldDirection;
#include <common>
void main() {
	vec3 direction = normalize( vWorldDirection );
	vec2 sampleUV = equirectUv( direction );
	gl_FragColor = texture2D( tEquirect, sampleUV );
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
}`,$d=`uniform float scale;
attribute float lineDistance;
varying float vLineDistance;
#include <common>
#include <uv_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <morphtarget_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	vLineDistance = scale * lineDistance;
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	#include <fog_vertex>
}`,Jd=`uniform vec3 diffuse;
uniform float opacity;
uniform float dashSize;
uniform float totalSize;
varying float vLineDistance;
#include <common>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <fog_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	if ( mod( vLineDistance, totalSize ) > dashSize ) {
		discard;
	}
	vec3 outgoingLight = vec3( 0.0 );
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	outgoingLight = diffuseColor.rgb;
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
}`,Qd=`#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <envmap_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#if defined ( USE_ENVMAP ) || defined ( USE_SKINNING )
		#include <beginnormal_vertex>
		#include <morphnormal_vertex>
		#include <skinbase_vertex>
		#include <skinnormal_vertex>
		#include <defaultnormal_vertex>
	#endif
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	#include <worldpos_vertex>
	#include <envmap_vertex>
	#include <fog_vertex>
}`,jd=`uniform vec3 diffuse;
uniform float opacity;
#ifndef FLAT_SHADED
	varying vec3 vNormal;
#endif
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <aomap_pars_fragment>
#include <lightmap_pars_fragment>
#include <envmap_common_pars_fragment>
#include <envmap_pars_fragment>
#include <fog_pars_fragment>
#include <specularmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <specularmap_fragment>
	ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
	#ifdef USE_LIGHTMAP
		vec4 lightMapTexel = texture2D( lightMap, vLightMapUv );
		reflectedLight.indirectDiffuse += lightMapTexel.rgb * lightMapIntensity * RECIPROCAL_PI;
	#else
		reflectedLight.indirectDiffuse += vec3( 1.0 );
	#endif
	#include <aomap_fragment>
	reflectedLight.indirectDiffuse *= diffuseColor.rgb;
	vec3 outgoingLight = reflectedLight.indirectDiffuse;
	#include <envmap_fragment>
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,tf=`#define LAMBERT
varying vec3 vViewPosition;
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <envmap_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <shadowmap_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	vViewPosition = - mvPosition.xyz;
	#include <worldpos_vertex>
	#include <envmap_vertex>
	#include <shadowmap_vertex>
	#include <fog_vertex>
}`,ef=`#define LAMBERT
uniform vec3 diffuse;
uniform vec3 emissive;
uniform float opacity;
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <aomap_pars_fragment>
#include <lightmap_pars_fragment>
#include <emissivemap_pars_fragment>
#include <cube_uv_reflection_fragment>
#include <envmap_common_pars_fragment>
#include <envmap_pars_fragment>
#include <envmap_physical_pars_fragment>
#include <fog_pars_fragment>
#include <bsdfs>
#include <lights_pars_begin>
#include <normal_pars_fragment>
#include <lights_lambert_pars_fragment>
#include <shadowmap_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <specularmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
	vec3 totalEmissiveRadiance = emissive;
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <specularmap_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	#include <emissivemap_fragment>
	#include <lights_lambert_fragment>
	#include <lights_fragment_begin>
	#include <lights_fragment_maps>
	#include <lights_fragment_end>
	#include <aomap_fragment>
	vec3 outgoingLight = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse + totalEmissiveRadiance;
	#include <envmap_fragment>
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,nf=`#define MATCAP
varying vec3 vViewPosition;
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <color_pars_vertex>
#include <displacementmap_pars_vertex>
#include <fog_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	#include <fog_vertex>
	vViewPosition = - mvPosition.xyz;
}`,rf=`#define MATCAP
uniform vec3 diffuse;
uniform float opacity;
uniform sampler2D matcap;
varying vec3 vViewPosition;
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <fog_pars_fragment>
#include <normal_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	vec3 viewDir = normalize( vViewPosition );
	vec3 x = normalize( vec3( viewDir.z, 0.0, - viewDir.x ) );
	vec3 y = cross( viewDir, x );
	vec2 uv = vec2( dot( x, normal ), dot( y, normal ) ) * 0.495 + 0.5;
	#ifdef USE_MATCAP
		vec4 matcapColor = texture2D( matcap, uv );
	#else
		vec4 matcapColor = vec4( vec3( mix( 0.2, 0.8, uv.y ) ), 1.0 );
	#endif
	vec3 outgoingLight = diffuseColor.rgb * matcapColor.rgb;
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,sf=`#define NORMAL
#if defined( FLAT_SHADED ) || defined( USE_BUMPMAP ) || defined( USE_NORMALMAP_TANGENTSPACE )
	varying vec3 vViewPosition;
#endif
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphinstance_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
#if defined( FLAT_SHADED ) || defined( USE_BUMPMAP ) || defined( USE_NORMALMAP_TANGENTSPACE )
	vViewPosition = - mvPosition.xyz;
#endif
}`,af=`#define NORMAL
uniform float opacity;
#if defined( FLAT_SHADED ) || defined( USE_BUMPMAP ) || defined( USE_NORMALMAP_TANGENTSPACE )
	varying vec3 vViewPosition;
#endif
#include <uv_pars_fragment>
#include <normal_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( 0.0, 0.0, 0.0, opacity );
	#include <clipping_planes_fragment>
	#include <logdepthbuf_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	gl_FragColor = vec4( normalize( normal ) * 0.5 + 0.5, diffuseColor.a );
	#ifdef OPAQUE
		gl_FragColor.a = 1.0;
	#endif
}`,of=`#define PHONG
varying vec3 vViewPosition;
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <envmap_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <shadowmap_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphinstance_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	vViewPosition = - mvPosition.xyz;
	#include <worldpos_vertex>
	#include <envmap_vertex>
	#include <shadowmap_vertex>
	#include <fog_vertex>
}`,lf=`#define PHONG
uniform vec3 diffuse;
uniform vec3 emissive;
uniform vec3 specular;
uniform float shininess;
uniform float opacity;
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <aomap_pars_fragment>
#include <lightmap_pars_fragment>
#include <emissivemap_pars_fragment>
#include <cube_uv_reflection_fragment>
#include <envmap_common_pars_fragment>
#include <envmap_pars_fragment>
#include <envmap_physical_pars_fragment>
#include <fog_pars_fragment>
#include <bsdfs>
#include <lights_pars_begin>
#include <normal_pars_fragment>
#include <lights_phong_pars_fragment>
#include <shadowmap_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <specularmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
	vec3 totalEmissiveRadiance = emissive;
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <specularmap_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	#include <emissivemap_fragment>
	#include <lights_phong_fragment>
	#include <lights_fragment_begin>
	#include <lights_fragment_maps>
	#include <lights_fragment_end>
	#include <aomap_fragment>
	vec3 outgoingLight = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse + reflectedLight.directSpecular + reflectedLight.indirectSpecular + totalEmissiveRadiance;
	#include <envmap_fragment>
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,cf=`#define STANDARD
varying vec3 vViewPosition;
#ifdef USE_TRANSMISSION
	varying vec3 vWorldPosition;
#endif
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <shadowmap_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	vViewPosition = - mvPosition.xyz;
	#include <worldpos_vertex>
	#include <shadowmap_vertex>
	#include <fog_vertex>
#ifdef USE_TRANSMISSION
	vWorldPosition = worldPosition.xyz;
#endif
}`,hf=`#define STANDARD
#ifdef PHYSICAL
	#define IOR
	#define USE_SPECULAR
#endif
uniform vec3 diffuse;
uniform vec3 emissive;
uniform float roughness;
uniform float metalness;
uniform float opacity;
#ifdef IOR
	uniform float ior;
#endif
#ifdef USE_SPECULAR
	uniform float specularIntensity;
	uniform vec3 specularColor;
	#ifdef USE_SPECULAR_COLORMAP
		uniform sampler2D specularColorMap;
	#endif
	#ifdef USE_SPECULAR_INTENSITYMAP
		uniform sampler2D specularIntensityMap;
	#endif
#endif
#ifdef USE_CLEARCOAT
	uniform float clearcoat;
	uniform float clearcoatRoughness;
#endif
#ifdef USE_DISPERSION
	uniform float dispersion;
#endif
#ifdef USE_RETROREFLECTION
	uniform float retroreflectivity;
#endif
#ifdef USE_IRIDESCENCE
	uniform float iridescence;
	uniform float iridescenceIOR;
	uniform float iridescenceThicknessMinimum;
	uniform float iridescenceThicknessMaximum;
#endif
#ifdef USE_SHEEN
	uniform vec3 sheenColor;
	uniform float sheenRoughness;
	#ifdef USE_SHEEN_COLORMAP
		uniform sampler2D sheenColorMap;
	#endif
	#ifdef USE_SHEEN_ROUGHNESSMAP
		uniform sampler2D sheenRoughnessMap;
	#endif
#endif
#ifdef USE_ANISOTROPY
	uniform vec2 anisotropyVector;
	#ifdef USE_ANISOTROPYMAP
		uniform sampler2D anisotropyMap;
	#endif
#endif
varying vec3 vViewPosition;
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <aomap_pars_fragment>
#include <lightmap_pars_fragment>
#include <emissivemap_pars_fragment>
#include <iridescence_fragment>
#include <cube_uv_reflection_fragment>
#include <envmap_common_pars_fragment>
#include <envmap_physical_pars_fragment>
#include <fog_pars_fragment>
#include <lights_pars_begin>
#include <normal_pars_fragment>
#include <lights_physical_pars_fragment>
#include <transmission_pars_fragment>
#include <shadowmap_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <clearcoat_pars_fragment>
#include <iridescence_pars_fragment>
#include <roughnessmap_pars_fragment>
#include <metalnessmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
	vec3 totalEmissiveRadiance = emissive;
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <roughnessmap_fragment>
	#include <metalnessmap_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	#include <clearcoat_normal_fragment_begin>
	#include <clearcoat_normal_fragment_maps>
	#include <emissivemap_fragment>
	#include <lights_physical_fragment>
	#include <lights_fragment_begin>
	#include <lights_fragment_maps>
	#include <lights_fragment_end>
	#include <aomap_fragment>
	vec3 totalDiffuse = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse;
	vec3 totalSpecular = reflectedLight.directSpecular + reflectedLight.indirectSpecular;
	#include <transmission_fragment>
	vec3 outgoingLight = totalDiffuse + totalSpecular + totalEmissiveRadiance;
	#ifdef USE_SHEEN
 
		outgoingLight = outgoingLight + sheenSpecularDirect + sheenSpecularIndirect;
 
 	#endif
	#ifdef USE_CLEARCOAT
		float dotNVcc = saturate( dot( geometryClearcoatNormal, geometryViewDir ) );
		vec3 Fcc = F_Schlick( material.clearcoatF0, material.clearcoatF90, dotNVcc );
		outgoingLight = outgoingLight * ( 1.0 - material.clearcoat * Fcc ) + ( clearcoatSpecularDirect + clearcoatSpecularIndirect ) * material.clearcoat;
	#endif
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,uf=`#define TOON
varying vec3 vViewPosition;
#include <common>
#include <batching_pars_vertex>
#include <uv_pars_vertex>
#include <displacementmap_pars_vertex>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <normal_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <shadowmap_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <normal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <displacementmap_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	vViewPosition = - mvPosition.xyz;
	#include <worldpos_vertex>
	#include <shadowmap_vertex>
	#include <fog_vertex>
}`,df=`#define TOON
uniform vec3 diffuse;
uniform vec3 emissive;
uniform float opacity;
#include <common>
#include <dithering_pars_fragment>
#include <color_pars_fragment>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <aomap_pars_fragment>
#include <lightmap_pars_fragment>
#include <emissivemap_pars_fragment>
#include <gradientmap_pars_fragment>
#include <fog_pars_fragment>
#include <bsdfs>
#include <lights_pars_begin>
#include <normal_pars_fragment>
#include <lights_toon_pars_fragment>
#include <shadowmap_pars_fragment>
#include <bumpmap_pars_fragment>
#include <normalmap_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	ReflectedLight reflectedLight = ReflectedLight( vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ), vec3( 0.0 ) );
	vec3 totalEmissiveRadiance = emissive;
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <color_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	#include <normal_fragment_begin>
	#include <normal_fragment_maps>
	#include <emissivemap_fragment>
	#include <lights_toon_fragment>
	#include <lights_fragment_begin>
	#include <lights_fragment_maps>
	#include <lights_fragment_end>
	#include <aomap_fragment>
	vec3 outgoingLight = reflectedLight.directDiffuse + reflectedLight.indirectDiffuse + totalEmissiveRadiance;
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
	#include <dithering_fragment>
}`,ff=`uniform float size;
uniform float scale;
#include <common>
#include <color_pars_vertex>
#include <fog_pars_vertex>
#include <morphtarget_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
#ifdef USE_POINTS_UV
	varying vec2 vUv;
	uniform mat3 uvTransform;
#endif
void main() {
	#ifdef USE_POINTS_UV
		vUv = ( uvTransform * vec3( uv, 1 ) ).xy;
	#endif
	#include <color_vertex>
	#include <morphinstance_vertex>
	#include <morphcolor_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <project_vertex>
	gl_PointSize = size;
	#ifdef USE_SIZEATTENUATION
		bool isPerspective = isPerspectiveMatrix( projectionMatrix );
		if ( isPerspective ) gl_PointSize *= ( scale / - mvPosition.z );
	#endif
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	#include <worldpos_vertex>
	#include <fog_vertex>
}`,pf=`uniform vec3 diffuse;
uniform float opacity;
#include <common>
#include <color_pars_fragment>
#include <map_particle_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <fog_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	vec3 outgoingLight = vec3( 0.0 );
	#include <logdepthbuf_fragment>
	#include <map_particle_fragment>
	#include <color_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	outgoingLight = diffuseColor.rgb;
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
}`,mf=`#include <common>
#include <batching_pars_vertex>
#include <fog_pars_vertex>
#include <morphtarget_pars_vertex>
#include <skinning_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <shadowmap_pars_vertex>
void main() {
	#include <batching_vertex>
	#include <beginnormal_vertex>
	#include <morphinstance_vertex>
	#include <morphnormal_vertex>
	#include <skinbase_vertex>
	#include <skinnormal_vertex>
	#include <defaultnormal_vertex>
	#include <begin_vertex>
	#include <morphtarget_vertex>
	#include <skinning_vertex>
	#include <project_vertex>
	#include <logdepthbuf_vertex>
	#include <worldpos_vertex>
	#include <shadowmap_vertex>
	#include <fog_vertex>
}`,_f=`uniform vec3 color;
uniform float opacity;
#include <common>
#include <fog_pars_fragment>
#include <bsdfs>
#include <lights_pars_begin>
#include <logdepthbuf_pars_fragment>
#include <shadowmap_pars_fragment>
#include <shadowmask_pars_fragment>
void main() {
	#include <logdepthbuf_fragment>
	gl_FragColor = vec4( color, opacity * ( 1.0 - getShadowMask() ) );
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
	#include <premultiplied_alpha_fragment>
}`,gf=`uniform float rotation;
uniform vec2 center;
#include <common>
#include <uv_pars_vertex>
#include <fog_pars_vertex>
#include <logdepthbuf_pars_vertex>
#include <clipping_planes_pars_vertex>
void main() {
	#include <uv_vertex>
	vec4 mvPosition = modelViewMatrix[ 3 ];
	vec2 scale = vec2( length( modelMatrix[ 0 ].xyz ), length( modelMatrix[ 1 ].xyz ) );
	#ifndef USE_SIZEATTENUATION
		bool isPerspective = isPerspectiveMatrix( projectionMatrix );
		if ( isPerspective ) scale *= - mvPosition.z;
	#endif
	vec2 alignedPosition = ( position.xy - ( center - vec2( 0.5 ) ) ) * scale;
	vec2 rotatedPosition;
	rotatedPosition.x = cos( rotation ) * alignedPosition.x - sin( rotation ) * alignedPosition.y;
	rotatedPosition.y = sin( rotation ) * alignedPosition.x + cos( rotation ) * alignedPosition.y;
	mvPosition.xy += rotatedPosition;
	gl_Position = projectionMatrix * mvPosition;
	#include <logdepthbuf_vertex>
	#include <clipping_planes_vertex>
	#include <fog_vertex>
}`,xf=`uniform vec3 diffuse;
uniform float opacity;
#include <common>
#include <uv_pars_fragment>
#include <map_pars_fragment>
#include <alphamap_pars_fragment>
#include <alphatest_pars_fragment>
#include <alphahash_pars_fragment>
#include <fog_pars_fragment>
#include <logdepthbuf_pars_fragment>
#include <clipping_planes_pars_fragment>
void main() {
	vec4 diffuseColor = vec4( diffuse, opacity );
	#include <clipping_planes_fragment>
	vec3 outgoingLight = vec3( 0.0 );
	#include <logdepthbuf_fragment>
	#include <map_fragment>
	#include <alphamap_fragment>
	#include <alphatest_fragment>
	#include <alphahash_fragment>
	outgoingLight = diffuseColor.rgb;
	#include <opaque_fragment>
	#include <tonemapping_fragment>
	#include <colorspace_fragment>
	#include <fog_fragment>
}`,Nt={alphahash_fragment:Bh,alphahash_pars_fragment:zh,alphamap_fragment:Gh,alphamap_pars_fragment:Hh,alphatest_fragment:Vh,alphatest_pars_fragment:kh,aomap_fragment:Wh,aomap_pars_fragment:Xh,batching_pars_vertex:Yh,batching_vertex:qh,begin_vertex:Zh,beginnormal_vertex:Kh,bsdfs:$h,iridescence_fragment:Jh,bumpmap_pars_fragment:Qh,clipping_planes_fragment:jh,clipping_planes_pars_fragment:tu,clipping_planes_pars_vertex:eu,clipping_planes_vertex:nu,color_fragment:iu,color_pars_fragment:ru,color_pars_vertex:su,color_vertex:au,common:ou,cube_uv_reflection_fragment:lu,defaultnormal_vertex:cu,displacementmap_pars_vertex:hu,displacementmap_vertex:uu,emissivemap_fragment:du,emissivemap_pars_fragment:fu,colorspace_fragment:pu,colorspace_pars_fragment:mu,envmap_fragment:_u,envmap_common_pars_fragment:gu,envmap_pars_fragment:xu,envmap_pars_vertex:vu,envmap_physical_pars_fragment:Pu,envmap_vertex:Su,fog_vertex:Mu,fog_pars_vertex:Eu,fog_fragment:yu,fog_pars_fragment:bu,gradientmap_pars_fragment:Tu,lightmap_pars_fragment:Au,lights_lambert_fragment:wu,lights_lambert_pars_fragment:Ru,lights_pars_begin:Cu,lights_toon_fragment:Du,lights_toon_pars_fragment:Lu,lights_phong_fragment:Iu,lights_phong_pars_fragment:Uu,lights_physical_fragment:Nu,lights_physical_pars_fragment:Fu,lights_fragment_begin:Ou,lights_fragment_maps:Bu,lights_fragment_end:zu,lightprobes_pars_fragment:Gu,logdepthbuf_fragment:Hu,logdepthbuf_pars_fragment:Vu,logdepthbuf_pars_vertex:ku,logdepthbuf_vertex:Wu,map_fragment:Xu,map_pars_fragment:Yu,map_particle_fragment:qu,map_particle_pars_fragment:Zu,metalnessmap_fragment:Ku,metalnessmap_pars_fragment:$u,morphinstance_vertex:Ju,morphcolor_vertex:Qu,morphnormal_vertex:ju,morphtarget_pars_vertex:td,morphtarget_vertex:ed,normal_fragment_begin:nd,normal_fragment_maps:id,normal_pars_fragment:rd,normal_pars_vertex:sd,normal_vertex:ad,normalmap_pars_fragment:od,clearcoat_normal_fragment_begin:ld,clearcoat_normal_fragment_maps:cd,clearcoat_pars_fragment:hd,iridescence_pars_fragment:ud,opaque_fragment:dd,packing:fd,premultiplied_alpha_fragment:pd,project_vertex:md,dithering_fragment:_d,dithering_pars_fragment:gd,roughnessmap_fragment:xd,roughnessmap_pars_fragment:vd,shadowmap_pars_fragment:Sd,shadowmap_pars_vertex:Md,shadowmap_vertex:Ed,shadowmask_pars_fragment:yd,skinbase_vertex:bd,skinning_pars_vertex:Td,skinning_vertex:Ad,skinnormal_vertex:wd,specularmap_fragment:Rd,specularmap_pars_fragment:Cd,tonemapping_fragment:Pd,tonemapping_pars_fragment:Dd,transmission_fragment:Ld,transmission_pars_fragment:Id,uv_pars_fragment:Ud,uv_pars_vertex:Nd,uv_vertex:Fd,worldpos_vertex:Od,background_vert:Bd,background_frag:zd,backgroundCube_vert:Gd,backgroundCube_frag:Hd,cube_vert:Vd,cube_frag:kd,depth_vert:Wd,depth_frag:Xd,distance_vert:Yd,distance_frag:qd,equirect_vert:Zd,equirect_frag:Kd,linedashed_vert:$d,linedashed_frag:Jd,meshbasic_vert:Qd,meshbasic_frag:jd,meshlambert_vert:tf,meshlambert_frag:ef,meshmatcap_vert:nf,meshmatcap_frag:rf,meshnormal_vert:sf,meshnormal_frag:af,meshphong_vert:of,meshphong_frag:lf,meshphysical_vert:cf,meshphysical_frag:hf,meshtoon_vert:uf,meshtoon_frag:df,points_vert:ff,points_frag:pf,shadow_vert:mf,shadow_frag:_f,sprite_vert:gf,sprite_frag:xf},ht={common:{diffuse:{value:new Gt(16777215)},opacity:{value:1},map:{value:null},mapTransform:{value:new Dt},alphaMap:{value:null},alphaMapTransform:{value:new Dt},alphaTest:{value:0}},specularmap:{specularMap:{value:null},specularMapTransform:{value:new Dt}},envmap:{envMap:{value:null},envMapRotation:{value:new Dt},reflectivity:{value:1},ior:{value:1.5},refractionRatio:{value:.98},dfgLUT:{value:null}},aomap:{aoMap:{value:null},aoMapIntensity:{value:1},aoMapTransform:{value:new Dt}},lightmap:{lightMap:{value:null},lightMapIntensity:{value:1},lightMapTransform:{value:new Dt}},bumpmap:{bumpMap:{value:null},bumpMapTransform:{value:new Dt},bumpScale:{value:1}},normalmap:{normalMap:{value:null},normalMapTransform:{value:new Dt},normalScale:{value:new Ct(1,1)}},displacementmap:{displacementMap:{value:null},displacementMapTransform:{value:new Dt},displacementScale:{value:1},displacementBias:{value:0}},emissivemap:{emissiveMap:{value:null},emissiveMapTransform:{value:new Dt}},metalnessmap:{metalnessMap:{value:null},metalnessMapTransform:{value:new Dt}},roughnessmap:{roughnessMap:{value:null},roughnessMapTransform:{value:new Dt}},gradientmap:{gradientMap:{value:null}},fog:{fogDensity:{value:25e-5},fogNear:{value:1},fogFar:{value:2e3},fogColor:{value:new Gt(16777215)}},lights:{ambientLightColor:{value:[]},lightProbe:{value:[]},sunLights:{value:[],properties:{direction:{},color:{}}},sunLightShadows:{value:[],properties:{shadowIntensity:1,shadowBias:{},shadowNormalBias:{},shadowRadius:{},shadowMapSize:{}}},sunShadowMatrix:{value:[]},sunShadowCascade:{value:[]},directionalLights:{value:[],properties:{direction:{},color:{}}},directionalLightShadows:{value:[],properties:{shadowIntensity:1,shadowBias:{},shadowNormalBias:{},shadowRadius:{},shadowMapSize:{}}},directionalShadowMatrix:{value:[]},spotLights:{value:[],properties:{color:{},position:{},direction:{},distance:{},coneCos:{},penumbraCos:{},decay:{}}},spotLightShadows:{value:[],properties:{shadowIntensity:1,shadowBias:{},shadowNormalBias:{},shadowRadius:{},shadowMapSize:{}}},spotLightMap:{value:[]},spotLightMatrix:{value:[]},pointLights:{value:[],properties:{color:{},position:{},decay:{},distance:{}}},pointLightShadows:{value:[],properties:{shadowIntensity:1,shadowBias:{},shadowNormalBias:{},shadowRadius:{},shadowMapSize:{},shadowCameraNear:{},shadowCameraFar:{}}},pointShadowMatrix:{value:[]},hemisphereLights:{value:[],properties:{direction:{},skyColor:{},groundColor:{}}},rectAreaLights:{value:[],properties:{color:{},position:{},width:{},height:{}}},ltc_1:{value:null},ltc_2:{value:null},probesSH:{value:null},probesMin:{value:new I},probesMax:{value:new I},probesResolution:{value:new I}},points:{diffuse:{value:new Gt(16777215)},opacity:{value:1},size:{value:1},scale:{value:1},map:{value:null},alphaMap:{value:null},alphaMapTransform:{value:new Dt},alphaTest:{value:0},uvTransform:{value:new Dt}},sprite:{diffuse:{value:new Gt(16777215)},opacity:{value:1},center:{value:new Ct(.5,.5)},rotation:{value:0},map:{value:null},mapTransform:{value:new Dt},alphaMap:{value:null},alphaMapTransform:{value:new Dt},alphaTest:{value:0}}},sn={basic:{uniforms:Pe([ht.common,ht.specularmap,ht.envmap,ht.aomap,ht.lightmap,ht.fog]),vertexShader:Nt.meshbasic_vert,fragmentShader:Nt.meshbasic_frag},lambert:{uniforms:Pe([ht.common,ht.specularmap,ht.envmap,ht.aomap,ht.lightmap,ht.emissivemap,ht.bumpmap,ht.normalmap,ht.displacementmap,ht.fog,ht.lights,{emissive:{value:new Gt(0)},envMapIntensity:{value:1}}]),vertexShader:Nt.meshlambert_vert,fragmentShader:Nt.meshlambert_frag},phong:{uniforms:Pe([ht.common,ht.specularmap,ht.envmap,ht.aomap,ht.lightmap,ht.emissivemap,ht.bumpmap,ht.normalmap,ht.displacementmap,ht.fog,ht.lights,{emissive:{value:new Gt(0)},specular:{value:new Gt(1118481)},shininess:{value:30},envMapIntensity:{value:1}}]),vertexShader:Nt.meshphong_vert,fragmentShader:Nt.meshphong_frag},standard:{uniforms:Pe([ht.common,ht.envmap,ht.aomap,ht.lightmap,ht.emissivemap,ht.bumpmap,ht.normalmap,ht.displacementmap,ht.roughnessmap,ht.metalnessmap,ht.fog,ht.lights,{emissive:{value:new Gt(0)},roughness:{value:1},metalness:{value:0},envMapIntensity:{value:1}}]),vertexShader:Nt.meshphysical_vert,fragmentShader:Nt.meshphysical_frag},toon:{uniforms:Pe([ht.common,ht.aomap,ht.lightmap,ht.emissivemap,ht.bumpmap,ht.normalmap,ht.displacementmap,ht.gradientmap,ht.fog,ht.lights,{emissive:{value:new Gt(0)}}]),vertexShader:Nt.meshtoon_vert,fragmentShader:Nt.meshtoon_frag},matcap:{uniforms:Pe([ht.common,ht.bumpmap,ht.normalmap,ht.displacementmap,ht.fog,{matcap:{value:null}}]),vertexShader:Nt.meshmatcap_vert,fragmentShader:Nt.meshmatcap_frag},points:{uniforms:Pe([ht.points,ht.fog]),vertexShader:Nt.points_vert,fragmentShader:Nt.points_frag},dashed:{uniforms:Pe([ht.common,ht.fog,{scale:{value:1},dashSize:{value:1},totalSize:{value:2}}]),vertexShader:Nt.linedashed_vert,fragmentShader:Nt.linedashed_frag},depth:{uniforms:Pe([ht.common,ht.displacementmap]),vertexShader:Nt.depth_vert,fragmentShader:Nt.depth_frag},normal:{uniforms:Pe([ht.common,ht.bumpmap,ht.normalmap,ht.displacementmap,{opacity:{value:1}}]),vertexShader:Nt.meshnormal_vert,fragmentShader:Nt.meshnormal_frag},sprite:{uniforms:Pe([ht.sprite,ht.fog]),vertexShader:Nt.sprite_vert,fragmentShader:Nt.sprite_frag},background:{uniforms:{uvTransform:{value:new Dt},t2D:{value:null},backgroundIntensity:{value:1}},vertexShader:Nt.background_vert,fragmentShader:Nt.background_frag},backgroundCube:{uniforms:{envMap:{value:null},backgroundBlurriness:{value:0},backgroundIntensity:{value:1},backgroundRotation:{value:new Dt}},vertexShader:Nt.backgroundCube_vert,fragmentShader:Nt.backgroundCube_frag},cube:{uniforms:{tCube:{value:null},tFlip:{value:-1},opacity:{value:1}},vertexShader:Nt.cube_vert,fragmentShader:Nt.cube_frag},equirect:{uniforms:{tEquirect:{value:null}},vertexShader:Nt.equirect_vert,fragmentShader:Nt.equirect_frag},distance:{uniforms:Pe([ht.common,ht.displacementmap,{referencePosition:{value:new I},nearDistance:{value:1},farDistance:{value:1e3}}]),vertexShader:Nt.distance_vert,fragmentShader:Nt.distance_frag},shadow:{uniforms:Pe([ht.lights,ht.fog,{color:{value:new Gt(0)},opacity:{value:1}}]),vertexShader:Nt.shadow_vert,fragmentShader:Nt.shadow_frag}};sn.physical={uniforms:Pe([sn.standard.uniforms,{clearcoat:{value:0},clearcoatMap:{value:null},clearcoatMapTransform:{value:new Dt},clearcoatNormalMap:{value:null},clearcoatNormalMapTransform:{value:new Dt},clearcoatNormalScale:{value:new Ct(1,1)},clearcoatRoughness:{value:0},clearcoatRoughnessMap:{value:null},clearcoatRoughnessMapTransform:{value:new Dt},dispersion:{value:0},retroreflectivity:{value:0},iridescence:{value:0},iridescenceMap:{value:null},iridescenceMapTransform:{value:new Dt},iridescenceIOR:{value:1.3},iridescenceThicknessMinimum:{value:100},iridescenceThicknessMaximum:{value:400},iridescenceThicknessMap:{value:null},iridescenceThicknessMapTransform:{value:new Dt},sheen:{value:0},sheenColor:{value:new Gt(0)},sheenColorMap:{value:null},sheenColorMapTransform:{value:new Dt},sheenRoughness:{value:1},sheenRoughnessMap:{value:null},sheenRoughnessMapTransform:{value:new Dt},transmission:{value:0},transmissionMap:{value:null},transmissionMapTransform:{value:new Dt},transmissionSamplerSize:{value:new Ct},transmissionSamplerMap:{value:null},thickness:{value:0},thicknessMap:{value:null},thicknessMapTransform:{value:new Dt},attenuationDistance:{value:0},attenuationColor:{value:new Gt(0)},specularColor:{value:new Gt(1,1,1)},specularColorMap:{value:null},specularColorMapTransform:{value:new Dt},specularIntensity:{value:1},specularIntensityMap:{value:null},specularIntensityMapTransform:{value:new Dt},anisotropyVector:{value:new Ct},anisotropyMap:{value:null},anisotropyMapTransform:{value:new Dt}}]),vertexShader:Nt.meshphysical_vert,fragmentShader:Nt.meshphysical_frag};const vr={r:0,b:0,g:0},vf=new ee,Nl=new Dt;Nl.set(-1,0,0,0,1,0,0,0,1);function Sf(i,t,e,n,r,s){const a=new Gt(0);let o=r===!0?0:1,c,l,d=null,p=0,h=null;function _(A){let P=A.isScene===!0?A.background:null;if(P&&P.isTexture){const S=A.backgroundBlurriness>0;P=t.get(P,S)}return P}function v(A){let P=!1;const S=_(A);S===null?m(a,o):S&&S.isColor&&(m(S,1),P=!0);const b=i.xr.getEnvironmentBlendMode();b==="additive"?e.buffers.color.setClear(0,0,0,1,s):b==="alpha-blend"&&e.buffers.color.setClear(0,0,0,0,s),(i.autoClear||P)&&(e.buffers.depth.setTest(!0),e.buffers.depth.setMask(!0),e.buffers.color.setMask(!0),i.clear(i.autoClearColor,i.autoClearDepth,i.autoClearStencil))}function M(A,P){const S=_(P);S&&(S.isCubeTexture||S.mapping===Or)?(l===void 0&&(l=new je(new Vi(1,1,1),new hn({name:"BackgroundCubeMaterial",uniforms:vi(sn.backgroundCube.uniforms),vertexShader:sn.backgroundCube.vertexShader,fragmentShader:sn.backgroundCube.fragmentShader,side:Ue,depthTest:!1,depthWrite:!1,fog:!1,allowOverride:!1})),l.geometry.deleteAttribute("normal"),l.geometry.deleteAttribute("uv"),l.onBeforeRender=function(b,y,w){this.matrixWorld.copyPosition(w.matrixWorld)},Object.defineProperty(l.material,"envMap",{get:function(){return this.uniforms.envMap.value}}),n.update(l)),l.material.uniforms.envMap.value=S,l.material.uniforms.backgroundBlurriness.value=P.backgroundBlurriness,l.material.uniforms.backgroundIntensity.value=P.backgroundIntensity,l.material.uniforms.backgroundRotation.value.setFromMatrix4(vf.makeRotationFromEuler(P.backgroundRotation)).transpose(),S.isCubeTexture&&S.isRenderTargetTexture===!1&&l.material.uniforms.backgroundRotation.value.premultiply(Nl),l.material.toneMapped=Ht.getTransfer(S.colorSpace)!==$t,(d!==S||p!==S.version||h!==i.toneMapping)&&(l.material.needsUpdate=!0,d=S,p=S.version,h=i.toneMapping),l.layers.enableAll(),A.unshift(l,l.geometry,l.material,0,0,null)):S&&S.isTexture&&(c===void 0&&(c=new je(new zr(2,2),new hn({name:"BackgroundMaterial",uniforms:vi(sn.background.uniforms),vertexShader:sn.background.vertexShader,fragmentShader:sn.background.fragmentShader,side:kn,depthTest:!1,depthWrite:!1,fog:!1,allowOverride:!1})),c.geometry.deleteAttribute("normal"),Object.defineProperty(c.material,"map",{get:function(){return this.uniforms.t2D.value}}),n.update(c)),c.material.uniforms.t2D.value=S,c.material.uniforms.backgroundIntensity.value=P.backgroundIntensity,c.material.toneMapped=Ht.getTransfer(S.colorSpace)!==$t,S.matrixAutoUpdate===!0&&S.updateMatrix(),c.material.uniforms.uvTransform.value.copy(S.matrix),(d!==S||p!==S.version||h!==i.toneMapping)&&(c.material.needsUpdate=!0,d=S,p=S.version,h=i.toneMapping),c.layers.enableAll(),A.unshift(c,c.geometry,c.material,0,0,null))}function m(A,P){A.getRGB(vr,Ll(i)),e.buffers.color.setClear(vr.r,vr.g,vr.b,P,s)}function u(){l!==void 0&&(l.geometry.dispose(),l.material.dispose(),l=void 0),c!==void 0&&(c.geometry.dispose(),c.material.dispose(),c=void 0)}return{getClearColor:function(){return a},setClearColor:function(A,P=1){a.set(A),o=P,m(a,o)},getClearAlpha:function(){return o},setClearAlpha:function(A){o=A,m(a,o)},render:v,addToRenderList:M,dispose:u}}function Mf(i,t){const e=i.getParameter(i.MAX_VERTEX_ATTRIBS),n={},r=h(null);let s=r,a=!1;function o(F,G,k,U,H){let K=!1;const Z=p(F,U,k,G);s!==Z&&(s=Z,l(s.object)),K=_(F,U,k,H),K&&v(F,U,k,H),H!==null&&t.update(H,i.ELEMENT_ARRAY_BUFFER),(K||a)&&(a=!1,S(F,G,k,U),H!==null&&i.bindBuffer(i.ELEMENT_ARRAY_BUFFER,t.get(H).buffer))}function c(){return i.createVertexArray()}function l(F){return i.bindVertexArray(F)}function d(F){return i.deleteVertexArray(F)}function p(F,G,k,U){const H=U.wireframe===!0;let K=n[G.id];K===void 0&&(K={},n[G.id]=K);const Z=F.isInstancedMesh===!0?F.id:0;let nt=K[Z];nt===void 0&&(nt={},K[Z]=nt);let X=nt[k.id];X===void 0&&(X={},nt[k.id]=X);let j=X[H];return j===void 0&&(j=h(c()),X[H]=j),j}function h(F){const G=[],k=[],U=[];for(let H=0;H<e;H++)G[H]=0,k[H]=0,U[H]=0;return{geometry:null,program:null,wireframe:!1,newAttributes:G,enabledAttributes:k,attributeDivisors:U,object:F,attributes:{},index:null}}function _(F,G,k,U){const H=s.attributes,K=G.attributes;let Z=0;const nt=k.getAttributes();for(const X in nt)if(nt[X].location>=0){const et=H[X];let At=K[X];if(At===void 0&&(X==="instanceMatrix"&&F.instanceMatrix&&(At=F.instanceMatrix),X==="instanceColor"&&F.instanceColor&&(At=F.instanceColor)),et===void 0||et.attribute!==At||At&&et.data!==At.data)return!0;Z++}return s.attributesNum!==Z||s.index!==U}function v(F,G,k,U){const H={},K=G.attributes;let Z=0;const nt=k.getAttributes();for(const X in nt)if(nt[X].location>=0){let et=K[X];et===void 0&&(X==="instanceMatrix"&&F.instanceMatrix&&(et=F.instanceMatrix),X==="instanceColor"&&F.instanceColor&&(et=F.instanceColor));const At={};At.attribute=et,et&&et.data&&(At.data=et.data),H[X]=At,Z++}s.attributes=H,s.attributesNum=Z,s.index=U}function M(){const F=s.newAttributes;for(let G=0,k=F.length;G<k;G++)F[G]=0}function m(F){u(F,0)}function u(F,G){const k=s.newAttributes,U=s.enabledAttributes,H=s.attributeDivisors;k[F]=1,U[F]===0&&(i.enableVertexAttribArray(F),U[F]=1),H[F]!==G&&(i.vertexAttribDivisor(F,G),H[F]=G)}function A(){const F=s.newAttributes,G=s.enabledAttributes;for(let k=0,U=G.length;k<U;k++)G[k]!==F[k]&&(i.disableVertexAttribArray(k),G[k]=0)}function P(F,G,k,U,H,K,Z){Z===!0?i.vertexAttribIPointer(F,G,k,H,K):i.vertexAttribPointer(F,G,k,U,H,K)}function S(F,G,k,U){M();const H=U.attributes,K=k.getAttributes(),Z=G.defaultAttributeValues;for(const nt in K){const X=K[nt];if(X.location>=0){let j=H[nt];if(j===void 0&&(nt==="instanceMatrix"&&F.instanceMatrix&&(j=F.instanceMatrix),nt==="instanceColor"&&F.instanceColor&&(j=F.instanceColor)),j!==void 0){const et=j.normalized,At=j.itemSize,bt=t.get(j);if(bt===void 0)continue;const ne=bt.buffer,Vt=bt.type,Yt=bt.bytesPerElement,Y=Vt===i.INT||Vt===i.UNSIGNED_INT||j.gpuType===pa;if(j.isInterleavedBufferAttribute){const Q=j.data,gt=Q.stride,Pt=j.offset;if(Q.isInstancedInterleavedBuffer){for(let mt=0;mt<X.locationSize;mt++)u(X.location+mt,Q.meshPerAttribute);F.isInstancedMesh!==!0&&U._maxInstanceCount===void 0&&(U._maxInstanceCount=Q.meshPerAttribute*Q.count)}else for(let mt=0;mt<X.locationSize;mt++)m(X.location+mt);i.bindBuffer(i.ARRAY_BUFFER,ne);for(let mt=0;mt<X.locationSize;mt++)P(X.location+mt,At/X.locationSize,Vt,et,gt*Yt,(Pt+At/X.locationSize*mt)*Yt,Y)}else{if(j.isInstancedBufferAttribute){for(let Q=0;Q<X.locationSize;Q++)u(X.location+Q,j.meshPerAttribute);F.isInstancedMesh!==!0&&U._maxInstanceCount===void 0&&(U._maxInstanceCount=j.meshPerAttribute*j.count)}else for(let Q=0;Q<X.locationSize;Q++)m(X.location+Q);i.bindBuffer(i.ARRAY_BUFFER,ne);for(let Q=0;Q<X.locationSize;Q++)P(X.location+Q,At/X.locationSize,Vt,et,At*Yt,At/X.locationSize*Q*Yt,Y)}}else if(Z!==void 0){const et=Z[nt];if(et!==void 0)switch(et.length){case 2:i.vertexAttrib2fv(X.location,et);break;case 3:i.vertexAttrib3fv(X.location,et);break;case 4:i.vertexAttrib4fv(X.location,et);break;default:i.vertexAttrib1fv(X.location,et)}}}}A()}function b(){T();for(const F in n){const G=n[F];for(const k in G){const U=G[k];for(const H in U){const K=U[H];for(const Z in K)d(K[Z].object),delete K[Z];delete U[H]}}delete n[F]}}function y(F){if(n[F.id]===void 0)return;const G=n[F.id];for(const k in G){const U=G[k];for(const H in U){const K=U[H];for(const Z in K)d(K[Z].object),delete K[Z];delete U[H]}}delete n[F.id]}function w(F){for(const G in n){const k=n[G];for(const U in k){const H=k[U];if(H[F.id]===void 0)continue;const K=H[F.id];for(const Z in K)d(K[Z].object),delete K[Z];delete H[F.id]}}}function x(F){for(const G in n){const k=n[G],U=F.isInstancedMesh===!0?F.id:0,H=k[U];if(H!==void 0){for(const K in H){const Z=H[K];for(const nt in Z)d(Z[nt].object),delete Z[nt];delete H[K]}delete k[U],Object.keys(k).length===0&&delete n[G]}}}function T(){L(),a=!0,s!==r&&(s=r,l(s.object))}function L(){r.geometry=null,r.program=null,r.wireframe=!1}return{setup:o,reset:T,resetDefaultState:L,dispose:b,releaseStatesOfGeometry:y,releaseStatesOfObject:x,releaseStatesOfProgram:w,initAttributes:M,enableAttribute:m,disableUnusedAttributes:A}}function Ef(i,t,e){let n;function r(c){n=c}function s(c,l){i.drawArrays(n,c,l),e.update(l,n,1)}function a(c,l,d){d!==0&&(i.drawArraysInstanced(n,c,l,d),e.update(l,n,d))}function o(c,l,d){if(d===0)return;t.get("WEBGL_multi_draw").multiDrawArraysWEBGL(n,c,0,l,0,d);let h=0;for(let _=0;_<d;_++)h+=l[_];e.update(h,n,1)}this.setMode=r,this.render=s,this.renderInstances=a,this.renderMultiDraw=o}function yf(i,t,e,n){let r;function s(){if(r!==void 0)return r;if(t.has("EXT_texture_filter_anisotropic")===!0){const w=t.get("EXT_texture_filter_anisotropic");r=i.getParameter(w.MAX_TEXTURE_MAX_ANISOTROPY_EXT)}else r=0;return r}function a(w){return!(w!==$e&&n.convert(w)!==i.getParameter(i.IMPLEMENTATION_COLOR_READ_FORMAT))}function o(w){const x=w===cn&&(t.has("EXT_color_buffer_half_float")||t.has("EXT_color_buffer_float"));return!(w!==Be&&w!==Ke&&!x&&n.convert(w)!==i.getParameter(i.IMPLEMENTATION_COLOR_READ_TYPE))}function c(w){if(w==="highp"){if(i.getShaderPrecisionFormat(i.VERTEX_SHADER,i.HIGH_FLOAT).precision>0&&i.getShaderPrecisionFormat(i.FRAGMENT_SHADER,i.HIGH_FLOAT).precision>0)return"highp";w="mediump"}return w==="mediump"&&i.getShaderPrecisionFormat(i.VERTEX_SHADER,i.MEDIUM_FLOAT).precision>0&&i.getShaderPrecisionFormat(i.FRAGMENT_SHADER,i.MEDIUM_FLOAT).precision>0?"mediump":"lowp"}let l=e.precision!==void 0?e.precision:"highp";const d=c(l);d!==l&&(Rt("WebGLRenderer:",l,"not supported, using",d,"instead."),l=d);const p=e.logarithmicDepthBuffer===!0,h=e.reversedDepthBuffer===!0&&t.has("EXT_clip_control");e.reversedDepthBuffer===!0&&h===!1&&Rt("WebGLRenderer: Unable to use reversed depth buffer due to missing EXT_clip_control extension. Fallback to default depth buffer.");const _=i.getParameter(i.MAX_TEXTURE_IMAGE_UNITS),v=i.getParameter(i.MAX_VERTEX_TEXTURE_IMAGE_UNITS),M=i.getParameter(i.MAX_TEXTURE_SIZE),m=i.getParameter(i.MAX_CUBE_MAP_TEXTURE_SIZE),u=i.getParameter(i.MAX_VERTEX_ATTRIBS),A=i.getParameter(i.MAX_VERTEX_UNIFORM_VECTORS),P=i.getParameter(i.MAX_VARYING_VECTORS),S=i.getParameter(i.MAX_FRAGMENT_UNIFORM_VECTORS),b=i.getParameter(i.MAX_SAMPLES),y=i.getParameter(i.SAMPLES);return{isWebGL2:!0,getMaxAnisotropy:s,getMaxPrecision:c,textureFormatReadable:a,textureTypeReadable:o,precision:l,logarithmicDepthBuffer:p,reversedDepthBuffer:h,maxTextures:_,maxVertexTextures:v,maxTextureSize:M,maxCubemapSize:m,maxAttributes:u,maxVertexUniforms:A,maxVaryings:P,maxFragmentUniforms:S,maxSamples:b,samples:y}}function bf(i){const t=this;let e=null,n=0,r=!1,s=!1;const a=new _n,o=new Dt,c={value:null,needsUpdate:!1};this.uniform=c,this.numPlanes=0,this.numIntersection=0,this.init=function(p,h){const _=p.length!==0||h||n!==0||r;return r=h,n=p.length,_},this.beginShadows=function(){s=!0,d(null)},this.endShadows=function(){s=!1},this.setGlobalState=function(p,h){e=d(p,h,0)},this.setState=function(p,h,_){const v=p.clippingPlanes,M=p.clipIntersection,m=p.clipShadows,u=i.get(p);if(!r||v===null||v.length===0||s&&!m)s?d(null):l();else{const A=s?0:n,P=A*4;let S=u.clippingState||null;c.value=S,S=d(v,h,P,_);for(let b=0;b!==P;++b)S[b]=e[b];u.clippingState=S,this.numIntersection=M?this.numPlanes:0,this.numPlanes+=A}};function l(){c.value!==e&&(c.value=e,c.needsUpdate=n>0),t.numPlanes=n,t.numIntersection=0}function d(p,h,_,v){const M=p!==null?p.length:0;let m=null;if(M!==0){if(m=c.value,v!==!0||m===null){const u=_+M*4,A=h.matrixWorldInverse;o.getNormalMatrix(A),(m===null||m.length<u)&&(m=new Float32Array(u));for(let P=0,S=_;P!==M;++P,S+=4)a.copy(p[P]).applyMatrix4(A,o),a.normal.toArray(m,S),m[S+3]=a.constant}c.value=m,c.needsUpdate=!0}return t.numPlanes=M,t.numIntersection=0,m}}const fi=4,Tf=6,Af=20,wf=256,Pi=new Ra,Lo=new Gt;let xs=null,vs=0,Ss=0,Ms=!1;const Rf=new I,Gn=new I;class Io{constructor(t){this._renderer=t,this._pingPongRenderTarget=null,this._lodMax=0,this._cubeSize=0,this._sizeLods=[],this._lodMeshes=[],this._backgroundBox=null,this._cubemapMaterial=null,this._equirectMaterial=null,this._blurMaterial=null,this._ggxMaterial=null}fromScene(t,e=0,n=.1,r=100,s={}){const{size:a=256,position:o=Rf}=s;xs=this._renderer.getRenderTarget(),vs=this._renderer.getActiveCubeFace(),Ss=this._renderer.getActiveMipmapLevel(),Ms=this._renderer.xr.enabled,this._renderer.xr.enabled=!1,this._setSize(a);const c=this._allocateTargets();return c.depthBuffer=!0,this._sceneToCubeUV(t,n,r,c,o),e>0&&this._blur(c,0,0,e),this._applyPMREM(c),this._cleanup(c),c}fromEquirectangular(t,e=null){return this._fromTexture(t,e)}fromCubemap(t,e=null){return this._fromTexture(t,e)}compileCubemapShader(){this._cubemapMaterial===null&&(this._cubemapMaterial=Fo(),this._compileMaterial(this._cubemapMaterial))}compileEquirectangularShader(){this._equirectMaterial===null&&(this._equirectMaterial=No(),this._compileMaterial(this._equirectMaterial))}dispose(){this._dispose(),this._cubemapMaterial!==null&&this._cubemapMaterial.dispose(),this._equirectMaterial!==null&&this._equirectMaterial.dispose(),this._backgroundBox!==null&&(this._backgroundBox.geometry.dispose(),this._backgroundBox.material.dispose())}_setSize(t){this._lodMax=Math.floor(Math.log2(t)),this._cubeSize=Math.pow(2,this._lodMax)}_dispose(){this._blurMaterial!==null&&this._blurMaterial.dispose(),this._ggxMaterial!==null&&this._ggxMaterial.dispose(),this._pingPongRenderTarget!==null&&this._pingPongRenderTarget.dispose();for(let t=0;t<this._lodMeshes.length;t++)this._lodMeshes[t].geometry.dispose()}_cleanup(t){this._renderer.setRenderTarget(xs,vs,Ss),this._renderer.xr.enabled=Ms,t.scissorTest=!1,hi(t,0,0,t.width,t.height)}_fromTexture(t,e){t.mapping===Wn||t.mapping===xi?this._setSize(t.image.length===0?16:t.image[0].width||t.image[0].image.width):this._setSize(t.image.width/4),xs=this._renderer.getRenderTarget(),vs=this._renderer.getActiveCubeFace(),Ss=this._renderer.getActiveMipmapLevel(),Ms=this._renderer.xr.enabled,this._renderer.xr.enabled=!1;const n=e||this._allocateTargets();return this._textureToCubeUV(t,n),this._applyPMREM(n),this._cleanup(n),n}_allocateTargets(){const t=3*Math.max(this._cubeSize,112),e=4*this._cubeSize,n={magFilter:Ae,minFilter:Ae,generateMipmaps:!1,type:cn,format:$e,colorSpace:Pr,depthBuffer:!1},r=Uo(t,e,n);if(this._pingPongRenderTarget===null||this._pingPongRenderTarget.width!==t||this._pingPongRenderTarget.height!==e){this._pingPongRenderTarget!==null&&this._dispose(),this._pingPongRenderTarget=Uo(t,e,n);const{_lodMax:s}=this;({lodMeshes:this._lodMeshes,sizeLods:this._sizeLods}=Cf(s)),this._blurMaterial=Df(s,t,e),this._ggxMaterial=Pf(s,t,e)}return r}_compileMaterial(t){const e=new je(new Ne,t);this._renderer.compile(e,Pi)}_sceneToCubeUV(t,e,n,r,s){const c=new qe(90,1,e,n),l=[1,-1,1,1,1,1],d=[1,1,1,-1,-1,-1],p=this._renderer,h=p.autoClear,_=p.toneMapping;p.getClearColor(Lo),p.toneMapping=on,p.autoClear=!1,p.state.buffers.depth.getReversed()&&(p.setRenderTarget(r),p.clearDepth(),p.setRenderTarget(null)),this._backgroundBox===null&&(this._backgroundBox=new je(new Vi,new wl({name:"PMREM.Background",side:Ue,depthWrite:!1,depthTest:!1})));const M=this._backgroundBox,m=M.material;let u=!1;const A=t.background;A?A.isColor&&(m.color.copy(A),t.background=null,u=!0):(m.color.copy(Lo),u=!0);for(let P=0;P<6;P++){const S=P%3;S===0?(c.up.set(0,l[P],0),c.position.set(s.x,s.y,s.z),c.lookAt(s.x+d[P],s.y,s.z)):S===1?(c.up.set(0,0,l[P]),c.position.set(s.x,s.y,s.z),c.lookAt(s.x,s.y+d[P],s.z)):(c.up.set(0,l[P],0),c.position.set(s.x,s.y,s.z),c.lookAt(s.x,s.y,s.z+d[P]));const b=this._cubeSize;hi(r,S*b,P>2?b:0,b,b),p.setRenderTarget(r),u&&p.render(M,c),p.render(t,c)}p.toneMapping=_,p.autoClear=h,t.background=A}_textureToCubeUV(t,e){const n=this._renderer,r=t.mapping===Wn||t.mapping===xi;r?(this._cubemapMaterial===null&&(this._cubemapMaterial=Fo()),this._cubemapMaterial.uniforms.flipEnvMap.value=t.isRenderTargetTexture===!1?-1:1):this._equirectMaterial===null&&(this._equirectMaterial=No());const s=r?this._cubemapMaterial:this._equirectMaterial,a=this._lodMeshes[0];a.material=s;const o=s.uniforms;o.envMap.value=t;const c=this._cubeSize;hi(e,0,0,3*c,2*c),n.setRenderTarget(e),n.render(a,Pi)}_applyPMREM(t){const e=this._renderer,n=e.autoClear;e.autoClear=!1;const r=this._lodMeshes.length;for(let s=1;s<r;s++)this._applyGGXFilter(t,s-1,s);e.autoClear=n}_applyGGXFilter(t,e,n){const r=this._renderer,s=this._pingPongRenderTarget,a=this._ggxMaterial,o=this._lodMeshes[n];o.material=a;const c=a.uniforms,l=n/(this._lodMeshes.length-1),d=e/(this._lodMeshes.length-1),p=Math.sqrt(l*l-d*d),h=l*1.25,_=p*h,{_lodMax:v}=this,M=this._sizeLods[n],m=3*M*(n>v-fi?n-v+fi:0),u=4*(this._cubeSize-M);c.envMap.value=t.texture,c.roughness.value=_,c.mipInt.value=v-e,hi(s,m,u,3*M,2*M),r.setRenderTarget(s),r.render(o,Pi),c.envMap.value=s.texture,c.roughness.value=0,c.mipInt.value=v-n,hi(t,m,u,3*M,2*M),r.setRenderTarget(t),r.render(o,Pi)}_blur(t,e,n,r){const s=this._pingPongRenderTarget,a=Math.min(r,Math.PI)/Math.SQRT2;this._blurPass(t,s,e,n,a),this._blurPass(s,t,n,n,a)}_blurPass(t,e,n,r,s){const a=this._renderer,o=this._blurMaterial,c=this._lodMeshes[r];c.material=o;const l=o.uniforms;l.envMap.value=t.texture,l.sigma.value=s,l.mipInt.value=this._lodMax-n;const d=this._sizeLods[r],p=3*d*(r>this._lodMax-fi?r-this._lodMax+fi:0),h=4*(this._cubeSize-d);hi(e,p,h,3*d,2*d),a.setRenderTarget(e),a.render(c,Pi)}}function Cf(i){const t=[],e=[];let n=i;const r=i-fi+1+Tf;for(let s=0;s<r;s++){const a=Math.pow(2,n);t.push(a);const o=1/(a-2),c=-o,l=1+o,d=[c,c,l,c,l,l,c,c,l,l,c,l],p=6,h=6,_=3,v=new Float32Array(_*h*p),M=new Float32Array(_*h*p);for(let u=0;u<p;u++){const A=u%3*2/3-1,P=u>2?0:-1,S=[A,P,0,A+2/3,P,0,A+2/3,P+1,0,A,P,0,A+2/3,P+1,0,A,P+1,0];v.set(S,_*h*u);for(let b=0;b<h;b++){const y=d[b*2]*2-1,w=d[b*2+1]*2-1;u===0?Gn.set(1,w,y):u===1?Gn.set(-y,1,-w):u===2?Gn.set(-y,w,1):u===3?Gn.set(-1,w,-y):u===4?Gn.set(-y,-1,w):Gn.set(y,w,-1),Gn.toArray(M,(u*h+b)*_)}}const m=new Ne;m.setAttribute("position",new Qe(v,_)),m.setAttribute("outputDirection",new Qe(M,_)),e.push(new je(m,null)),n>fi&&n--}return{lodMeshes:e,sizeLods:t}}function Uo(i,t,e){const n=new Je(i,t,e);return n.texture.mapping=Or,n.texture.name="PMREM.cubeUv",n.scissorTest=!0,n}function hi(i,t,e,n,r){i.viewport.set(t,e,n,r),i.scissor.set(t,e,n,r)}function Pf(i,t,e){return new hn({name:"PMREMGGXConvolution",defines:{GGX_SAMPLES:wf,CUBEUV_TEXEL_WIDTH:1/t,CUBEUV_TEXEL_HEIGHT:1/e,CUBEUV_MAX_MIP:`${i}.0`},uniforms:{envMap:{value:null},roughness:{value:0},mipInt:{value:0}},vertexShader:Gr(),fragmentShader:`

			precision highp float;
			precision highp int;

			varying vec3 vOutputDirection;

			uniform sampler2D envMap;
			uniform float roughness;
			uniform float mipInt;

			#define ENVMAP_TYPE_CUBE_UV
			#include <cube_uv_reflection_fragment>

			#define PI 3.14159265359

			// Van der Corput radical inverse
			float radicalInverse_VdC(uint bits) {
				bits = (bits << 16u) | (bits >> 16u);
				bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
				bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
				bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
				bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
				return float(bits) * 2.3283064365386963e-10; // / 0x100000000
			}

			// Hammersley sequence
			vec2 hammersley(uint i, uint N) {
				return vec2(float(i) / float(N), radicalInverse_VdC(i));
			}

			// GGX VNDF importance sampling (Eric Heitz 2018)
			// "Sampling the GGX Distribution of Visible Normals"
			// https://jcgt.org/published/0007/04/01/
			vec3 importanceSampleGGX_VNDF(vec2 Xi, vec3 V, float roughness) {
				float alpha = roughness * roughness;

				// Section 4.1: Orthonormal basis
				vec3 T1 = vec3(1.0, 0.0, 0.0);
				vec3 T2 = cross(V, T1);

				// Section 4.2: Parameterization of projected area
				float r = sqrt(Xi.x);
				float phi = 2.0 * PI * Xi.y;
				float t1 = r * cos(phi);
				float t2 = r * sin(phi);
				float s = 0.5 * (1.0 + V.z);
				t2 = (1.0 - s) * sqrt(1.0 - t1 * t1) + s * t2;

				// Section 4.3: Reprojection onto hemisphere
				vec3 Nh = t1 * T1 + t2 * T2 + sqrt(max(0.0, 1.0 - t1 * t1 - t2 * t2)) * V;

				// Section 3.4: Transform back to ellipsoid configuration
				return normalize(vec3(alpha * Nh.x, alpha * Nh.y, max(0.0, Nh.z)));
			}

			void main() {
				vec3 N = normalize(vOutputDirection);
				vec3 V = N; // Assume view direction equals normal for pre-filtering

				vec3 prefilteredColor = vec3(0.0);
				float totalWeight = 0.0;

				// For very low roughness, just sample the environment directly
				if (roughness < 0.001) {
					gl_FragColor = vec4(bilinearCubeUV(envMap, N, mipInt), 1.0);
					return;
				}

				// Tangent space basis for VNDF sampling
				vec3 up = abs(N.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
				vec3 tangent = normalize(cross(up, N));
				vec3 bitangent = cross(N, tangent);

				for(uint i = 0u; i < uint(GGX_SAMPLES); i++) {
					vec2 Xi = hammersley(i, uint(GGX_SAMPLES));

					// For PMREM, V = N, so in tangent space V is always (0, 0, 1)
					vec3 H_tangent = importanceSampleGGX_VNDF(Xi, vec3(0.0, 0.0, 1.0), roughness);

					// Transform H back to world space
					vec3 H = normalize(tangent * H_tangent.x + bitangent * H_tangent.y + N * H_tangent.z);
					vec3 L = normalize(2.0 * dot(V, H) * H - V);

					float NdotL = max(dot(N, L), 0.0);

					if(NdotL > 0.0) {
						// Sample environment at fixed mip level
						// VNDF importance sampling handles the distribution filtering
						vec3 sampleColor = bilinearCubeUV(envMap, L, mipInt);

						// Weight by NdotL for the split-sum approximation
						// VNDF PDF naturally accounts for the visible microfacet distribution
						prefilteredColor += sampleColor * NdotL;
						totalWeight += NdotL;
					}
				}

				if (totalWeight > 0.0) {
					prefilteredColor = prefilteredColor / totalWeight;
				}

				gl_FragColor = vec4(prefilteredColor, 1.0);
			}
		`,blending:vn,depthTest:!1,depthWrite:!1})}function Df(i,t,e){return new hn({name:"SphericalGaussianBlur",defines:{SAMPLES:Af,CUBEUV_TEXEL_WIDTH:1/t,CUBEUV_TEXEL_HEIGHT:1/e,CUBEUV_MAX_MIP:`${i}.0`},uniforms:{envMap:{value:null},sigma:{value:0},mipInt:{value:0}},vertexShader:Gr(),fragmentShader:`

			precision highp float;
			precision highp int;

			varying vec3 vOutputDirection;

			uniform sampler2D envMap;
			uniform float sigma;
			uniform float mipInt;

			#define ENVMAP_TYPE_CUBE_UV
			#include <cube_uv_reflection_fragment>

			#define PI 3.14159265359
			#define GOLDEN_ANGLE 2.39996322973

			void main() {

				if ( sigma == 0.0 ) {

					gl_FragColor = vec4( bilinearCubeUV( envMap, vOutputDirection, mipInt ), 1.0 );
					return;

				}

				vec3 outputDirection = normalize( vOutputDirection );

				vec3 up = abs( outputDirection.z ) < 0.999 ? vec3( 0.0, 0.0, 1.0 ) : vec3( 1.0, 0.0, 0.0 );
				vec3 tangent = normalize( cross( up, outputDirection ) );
				vec3 bitangent = cross( outputDirection, tangent );

				// Truncate the kernel at three standard deviations or at the antipode.
				float thetaMax = min( 3.0 * sigma, PI );
				float truncation = 1.0 - exp( - 0.5 * thetaMax * thetaMax / ( sigma * sigma ) );

				vec3 accumColor = vec3( 0.0 );
				float accumWeight = 0.0;

				for ( int i = 0; i < SAMPLES; i ++ ) {

					// Stratified inverse-CDF sampling of the Gaussian, placed on a golden-angle spiral.
					float stratum = ( float( i ) + 0.5 ) / float( SAMPLES );
					float theta = sigma * sqrt( - 2.0 * log( 1.0 - stratum * truncation ) );
					float phi = float( i ) * GOLDEN_ANGLE;

					vec3 offset = cos( phi ) * tangent + sin( phi ) * bitangent;
					vec3 sampleDirection = cos( theta ) * outputDirection + sin( theta ) * offset;

					// Correct the planar sample density to solid angle.
					float weight = sin( theta ) / theta;

					accumColor += weight * bilinearCubeUV( envMap, sampleDirection, mipInt );
					accumWeight += weight;

				}

				gl_FragColor = vec4( accumColor / accumWeight, 1.0 );

			}
		`,blending:vn,depthTest:!1,depthWrite:!1})}function No(){return new hn({name:"EquirectangularToCubeUV",uniforms:{envMap:{value:null}},vertexShader:Gr(),fragmentShader:`

			precision mediump float;
			precision mediump int;

			varying vec3 vOutputDirection;

			uniform sampler2D envMap;

			#include <common>

			void main() {

				vec3 outputDirection = normalize( vOutputDirection );
				vec2 uv = equirectUv( outputDirection );

				gl_FragColor = vec4( texture2D ( envMap, uv ).rgb, 1.0 );

			}
		`,blending:vn,depthTest:!1,depthWrite:!1})}function Fo(){return new hn({name:"CubemapToCubeUV",uniforms:{envMap:{value:null},flipEnvMap:{value:-1}},vertexShader:Gr(),fragmentShader:`

			precision mediump float;
			precision mediump int;

			uniform float flipEnvMap;

			varying vec3 vOutputDirection;

			uniform samplerCube envMap;

			void main() {

				gl_FragColor = textureCube( envMap, vec3( flipEnvMap * vOutputDirection.x, vOutputDirection.yz ) );

			}
		`,blending:vn,depthTest:!1,depthWrite:!1})}function Gr(){return`

		precision mediump float;
		precision mediump int;

		attribute vec3 outputDirection;

		varying vec3 vOutputDirection;

		void main() {

			vOutputDirection = outputDirection;
			gl_Position = vec4( position, 1.0 );

		}
	`}class Fl extends Je{constructor(t=1,e={}){super(t,t,e),this.isWebGLCubeRenderTarget=!0;const n={width:t,height:t,depth:1},r=[n,n,n,n,n,n];this.texture=new Cl(r),this._setTextureOptions(e),this.texture.isRenderTargetTexture=!0}fromEquirectangularTexture(t,e){this.texture.type=e.type,this.texture.colorSpace=e.colorSpace,this.texture.generateMipmaps=e.generateMipmaps,this.texture.minFilter=e.minFilter,this.texture.magFilter=e.magFilter;const n={uniforms:{tEquirect:{value:null}},vertexShader:`

				varying vec3 vWorldDirection;

				vec3 transformDirection( in vec3 dir, in mat4 matrix ) {

					return normalize( ( matrix * vec4( dir, 0.0 ) ).xyz );

				}

				void main() {

					vWorldDirection = transformDirection( position, modelMatrix );

					#include <begin_vertex>
					#include <project_vertex>

				}
			`,fragmentShader:`

				uniform sampler2D tEquirect;

				varying vec3 vWorldDirection;

				#include <common>

				void main() {

					vec3 direction = normalize( vWorldDirection );

					vec2 sampleUV = equirectUv( direction );

					gl_FragColor = texture2D( tEquirect, sampleUV );

				}
			`},r=new Vi(5,5,5),s=new hn({name:"CubemapFromEquirect",uniforms:vi(n.uniforms),vertexShader:n.vertexShader,fragmentShader:n.fragmentShader,side:Ue,blending:vn});s.uniforms.tEquirect.value=e;const a=new je(r,s),o=e.minFilter;return e.minFilter===Hn&&(e.minFilter=Ae),new Dh(1,10,this).update(t,a),e.minFilter=o,a.geometry.dispose(),a.material.dispose(),this}clear(t,e=!0,n=!0,r=!0){const s=t.getRenderTarget();for(let a=0;a<6;a++)t.setRenderTarget(this,a),t.clear(e,n,r);t.setRenderTarget(s)}}function Lf(i){let t=new WeakMap,e=new WeakMap,n=null;function r(h,_=!1){return h==null?null:_?a(h):s(h)}function s(h){if(h&&h.isTexture){const _=h.mapping;if(_===Wr||_===Xr)if(t.has(h)){const v=t.get(h).texture;return o(v,h.mapping)}else{const v=h.image;if(v&&v.height>0){const M=new Fl(v.height);return M.fromEquirectangularTexture(i,h),t.set(h,M),h.addEventListener("dispose",l),o(M.texture,h.mapping)}else return null}}return h}function a(h){if(h&&h.isTexture){const _=h.mapping,v=_===Wr||_===Xr,M=_===Wn||_===xi;if(v||M){let m=e.get(h);const u=m!==void 0?m.texture.pmremVersion:0;if(h.isRenderTargetTexture&&h.pmremVersion!==u)return n===null&&(n=new Io(i)),m=v?n.fromEquirectangular(h,m):n.fromCubemap(h,m),m.texture.pmremVersion=h.pmremVersion,e.set(h,m),m.texture;if(m!==void 0)return m.texture;{const A=h.image;return v&&A&&A.height>0||M&&A&&c(A)?(n===null&&(n=new Io(i)),m=v?n.fromEquirectangular(h):n.fromCubemap(h),m.texture.pmremVersion=h.pmremVersion,e.set(h,m),h.addEventListener("dispose",d),m.texture):null}}}return h}function o(h,_){return _===Wr?h.mapping=Wn:_===Xr&&(h.mapping=xi),h}function c(h){let _=0;const v=6;for(let M=0;M<v;M++)h[M]!==void 0&&_++;return _===v}function l(h){const _=h.target;_.removeEventListener("dispose",l);const v=t.get(_);v!==void 0&&(t.delete(_),v.dispose())}function d(h){const _=h.target;_.removeEventListener("dispose",d);const v=e.get(_);v!==void 0&&(e.delete(_),v.dispose())}function p(){t=new WeakMap,e=new WeakMap,n!==null&&(n.dispose(),n=null)}return{get:r,dispose:p}}function If(i){const t={};function e(n){if(t[n]!==void 0)return t[n];const r=i.getExtension(n);return t[n]=r,r}return{has:function(n){return e(n)!==null},init:function(){e("EXT_color_buffer_float"),e("WEBGL_clip_cull_distance"),e("OES_texture_float_linear"),e("EXT_color_buffer_half_float"),e("WEBGL_multisampled_render_to_texture"),e("WEBGL_render_shared_exponent")},get:function(n){const r=e(n);return r===null&&mi("WebGLRenderer: "+n+" extension not supported."),r}}}function Uf(i,t,e,n){const r={},s=new WeakMap;function a(p){const h=p.target;h.index!==null&&t.remove(h.index);for(const v in h.attributes)t.remove(h.attributes[v]);h.removeEventListener("dispose",a),delete r[h.id];const _=s.get(h);_&&(t.remove(_),s.delete(h)),n.releaseStatesOfGeometry(h),h.isInstancedBufferGeometry===!0&&delete h._maxInstanceCount,e.memory.geometries--}function o(p,h){return r[h.id]===!0||(h.addEventListener("dispose",a),r[h.id]=!0,e.memory.geometries++),h}function c(p){const h=p.attributes;for(const _ in h)t.update(h[_],i.ARRAY_BUFFER)}function l(p){const h=[],_=p.index,v=p.attributes.position;let M=0;if(v===void 0)return;if(_!==null){const A=_.array;M=_.version;for(let P=0,S=A.length;P<S;P+=3){const b=A[P+0],y=A[P+1],w=A[P+2];h.push(b,y,y,w,w,b)}}else{const A=v.array;M=v.version;for(let P=0,S=A.length/3-1;P<S;P+=3){const b=P+0,y=P+1,w=P+2;h.push(b,y,y,w,w,b)}}const m=new(v.count>=65535?Tl:bl)(h,1);m.version=M;const u=s.get(p);u&&t.remove(u),s.set(p,m)}function d(p){const h=s.get(p);if(h){const _=p.index;_!==null&&h.version<_.version&&l(p)}else l(p);return s.get(p)}return{get:o,update:c,getWireframeAttribute:d}}function Nf(i,t,e){let n;function r(p){n=p}let s,a;function o(p){s=p.type,a=p.bytesPerElement}function c(p,h){i.drawElements(n,h,s,p*a),e.update(h,n,1)}function l(p,h,_){_!==0&&(i.drawElementsInstanced(n,h,s,p*a,_),e.update(h,n,_))}function d(p,h,_){if(_===0)return;t.get("WEBGL_multi_draw").multiDrawElementsWEBGL(n,h,0,s,p,0,_);let M=0;for(let m=0;m<_;m++)M+=h[m];e.update(M,n,1)}this.setMode=r,this.setIndex=o,this.render=c,this.renderInstances=l,this.renderMultiDraw=d}function Ff(i){const t={geometries:0,textures:0},e={frame:0,calls:0,triangles:0,points:0,lines:0};function n(s,a,o){switch(e.calls++,a){case i.TRIANGLES:e.triangles+=o*(s/3);break;case i.LINES:e.lines+=o*(s/2);break;case i.LINE_STRIP:e.lines+=o*(s-1);break;case i.LINE_LOOP:e.lines+=o*s;break;case i.POINTS:e.points+=o*s;break;default:Wt("WebGLInfo: Unknown draw mode:",a);break}}function r(){e.calls=0,e.triangles=0,e.points=0,e.lines=0}return{memory:t,render:e,programs:null,autoReset:!0,reset:r,update:n}}function Of(i,t,e){const n=new WeakMap,r=new ce;function s(a,o,c){const l=a.morphTargetInfluences,d=o.morphAttributes.position||o.morphAttributes.normal||o.morphAttributes.color,p=d!==void 0?d.length:0;let h=n.get(o);if(h===void 0||h.count!==p){let T=function(){w.dispose(),n.delete(o),o.removeEventListener("dispose",T)};h!==void 0&&h.texture.dispose();const _=o.morphAttributes.position!==void 0,v=o.morphAttributes.normal!==void 0,M=o.morphAttributes.color!==void 0,m=o.morphAttributes.position||[],u=o.morphAttributes.normal||[],A=o.morphAttributes.color||[];let P=0;_===!0&&(P=1),v===!0&&(P=2),M===!0&&(P=3);let S=o.attributes.position.count*P,b=1;S>t.maxTextureSize&&(b=Math.ceil(S/t.maxTextureSize),S=t.maxTextureSize);const y=new Float32Array(S*b*4*p),w=new Ml(y,S,b,p);w.type=Ke,w.needsUpdate=!0;const x=P*4;for(let L=0;L<p;L++){const F=m[L],G=u[L],k=A[L],U=S*b*4*L;for(let H=0;H<F.count;H++){const K=H*x;_===!0&&(r.fromBufferAttribute(F,H),y[U+K+0]=r.x,y[U+K+1]=r.y,y[U+K+2]=r.z,y[U+K+3]=0),v===!0&&(r.fromBufferAttribute(G,H),y[U+K+4]=r.x,y[U+K+5]=r.y,y[U+K+6]=r.z,y[U+K+7]=0),M===!0&&(r.fromBufferAttribute(k,H),y[U+K+8]=r.x,y[U+K+9]=r.y,y[U+K+10]=r.z,y[U+K+11]=k.itemSize===4?r.w:1)}}h={count:p,texture:w,size:new Ct(S,b)},n.set(o,h),o.addEventListener("dispose",T)}if(a.isInstancedMesh===!0&&a.morphTexture!==null)c.getUniforms().setValue(i,"morphTexture",a.morphTexture,e);else{let _=0;for(let M=0;M<l.length;M++)_+=l[M];const v=o.morphTargetsRelative?1:1-_;c.getUniforms().setValue(i,"morphTargetBaseInfluence",v),c.getUniforms().setValue(i,"morphTargetInfluences",l)}c.getUniforms().setValue(i,"morphTargetsTexture",h.texture,e),c.getUniforms().setValue(i,"morphTargetsTextureSize",h.size)}return{update:s}}function Bf(i,t,e,n,r){let s=new WeakMap;function a(l){const d=r.render.frame,p=l.geometry,h=t.get(l,p);if(s.get(h)!==d&&(t.update(h),s.set(h,d)),l.isInstancedMesh&&(l.hasEventListener("dispose",c)===!1&&l.addEventListener("dispose",c),s.get(l)!==d&&(e.update(l.instanceMatrix,i.ARRAY_BUFFER),l.instanceColor!==null&&e.update(l.instanceColor,i.ARRAY_BUFFER),s.set(l,d))),l.isSkinnedMesh){const _=l.skeleton;s.get(_)!==d&&(_.update(),s.set(_,d))}return h}function o(){s=new WeakMap}function c(l){const d=l.target;d.removeEventListener("dispose",c),n.releaseStatesOfObject(d),e.remove(d.instanceMatrix),d.instanceColor!==null&&e.remove(d.instanceColor)}return{update:a,dispose:o}}const zf={[sl]:"LINEAR_TONE_MAPPING",[al]:"REINHARD_TONE_MAPPING",[ol]:"CINEON_TONE_MAPPING",[ll]:"ACES_FILMIC_TONE_MAPPING",[hl]:"AGX_TONE_MAPPING",[ul]:"NEUTRAL_TONE_MAPPING",[cl]:"CUSTOM_TONE_MAPPING"};function Gf(i,t,e,n,r,s){const a=new Je(t,e,{type:i,depthBuffer:r,stencilBuffer:s,samples:n?4:0,storeMultisampledDepthBuffer:!1,storeMultisampledStencilBuffer:!1,resolveDepthBuffer:!1,resolveStencilBuffer:!1});let o=null,c=null;const l=new Ne;l.setAttribute("position",new we([-1,3,0,-1,-1,0,3,-1,0],3)),l.setAttribute("uv",new we([0,2,0,0,2,0],2));const d=new Ah({uniforms:{tDiffuse:{value:null}},vertexShader:`
			precision highp float;

			uniform mat4 modelViewMatrix;
			uniform mat4 projectionMatrix;

			attribute vec3 position;
			attribute vec2 uv;

			varying vec2 vUv;

			void main() {
				vUv = uv;
				gl_Position = projectionMatrix * modelViewMatrix * vec4( position, 1.0 );
			}`,fragmentShader:`
			precision highp float;

			uniform sampler2D tDiffuse;

			varying vec2 vUv;

			#include <tonemapping_pars_fragment>
			#include <colorspace_pars_fragment>

			void main() {
				gl_FragColor = texture2D( tDiffuse, vUv );

				#ifdef LINEAR_TONE_MAPPING
					gl_FragColor.rgb = LinearToneMapping( gl_FragColor.rgb );
				#elif defined( REINHARD_TONE_MAPPING )
					gl_FragColor.rgb = ReinhardToneMapping( gl_FragColor.rgb );
				#elif defined( CINEON_TONE_MAPPING )
					gl_FragColor.rgb = CineonToneMapping( gl_FragColor.rgb );
				#elif defined( ACES_FILMIC_TONE_MAPPING )
					gl_FragColor.rgb = ACESFilmicToneMapping( gl_FragColor.rgb );
				#elif defined( AGX_TONE_MAPPING )
					gl_FragColor.rgb = AgXToneMapping( gl_FragColor.rgb );
				#elif defined( NEUTRAL_TONE_MAPPING )
					gl_FragColor.rgb = NeutralToneMapping( gl_FragColor.rgb );
				#elif defined( CUSTOM_TONE_MAPPING )
					gl_FragColor.rgb = CustomToneMapping( gl_FragColor.rgb );
				#endif

				#ifdef SRGB_TRANSFER
					gl_FragColor = sRGBTransferOETF( gl_FragColor );
				#endif
			}`,depthTest:!1,depthWrite:!1}),p=new je(l,d),h=new Ra(-1,1,1,-1,0,1);let _=null,v=null,M=!1,m,u=null,A=[],P=!1;this.setSize=function(S,b){a.setSize(S,b),o!==null&&o.setSize(S,b),c!==null&&c.setSize(S,b);for(let y=0;y<A.length;y++){const w=A[y];w.setSize&&w.setSize(S,b)}},this.setEffects=function(S){A=S,P=A.length>0&&A[0].isRenderPass===!0;const b=a.width,y=a.height;A.length>0&&o===null&&(o=new Je(b,y,{type:cn,depthBuffer:!1,stencilBuffer:!1}),c=new Je(b,y,{type:cn,depthBuffer:!1,stencilBuffer:!1}));for(let w=0;w<A.length;w++){const x=A[w];x.setSize&&x.setSize(b,y)}},this.begin=function(S,b){if(M||S.toneMapping===on&&A.length===0)return!1;if(u=b,b!==null){const y=b.width,w=b.height;(a.width!==y||a.height!==w)&&this.setSize(y,w)}return P===!1&&S.setRenderTarget(a),m=S.toneMapping,S.toneMapping=on,!0},this.hasRenderPass=function(){return P},this.end=function(S,b){S.toneMapping=m,M=!0;let y=a,w=o;for(let x=0;x<A.length;x++){const T=A[x];T.enabled!==!1&&(T.render(S,w,y,b),T.needsSwap!==!1&&(y=w,w=w===o?c:o))}if(_!==S.outputColorSpace||v!==S.toneMapping){_=S.outputColorSpace,v=S.toneMapping,d.defines={},Ht.getTransfer(_)===$t&&(d.defines.SRGB_TRANSFER="");const x=zf[v];x&&(d.defines[x]=""),d.needsUpdate=!0}d.uniforms.tDiffuse.value=y.texture,S.setRenderTarget(u),S.render(p,h),u=null,M=!1},this.isCompositing=function(){return M},this.dispose=function(){a.dispose(),o!==null&&o.dispose(),c!==null&&c.dispose(),l.dispose(),d.dispose()}}const Ol=new De,da=new Hi(1,1),Bl=new Ml,zl=new eh,Gl=new Cl,Oo=[],Bo=[],zo=new Float32Array(16),Go=new Float32Array(9),Ho=new Float32Array(4);function Ei(i,t,e){const n=i[0];if(n<=0||n>0)return i;const r=t*e;let s=Oo[r];if(s===void 0&&(s=new Float32Array(r),Oo[r]=s),t!==0){n.toArray(s,0);for(let a=1,o=0;a!==t;++a)o+=e,i[a].toArray(s,o)}return s}function _e(i,t){if(i.length!==t.length)return!1;for(let e=0,n=i.length;e<n;e++)if(i[e]!==t[e])return!1;return!0}function ge(i,t){for(let e=0,n=t.length;e<n;e++)i[e]=t[e]}function Hr(i,t){let e=Bo[t];e===void 0&&(e=new Int32Array(t),Bo[t]=e);for(let n=0;n!==t;++n)e[n]=i.allocateTextureUnit();return e}function Hf(i,t){const e=this.cache;e[0]!==t&&(i.uniform1f(this.addr,t),e[0]=t)}function Vf(i,t){const e=this.cache;if(t.x!==void 0)(e[0]!==t.x||e[1]!==t.y)&&(i.uniform2f(this.addr,t.x,t.y),e[0]=t.x,e[1]=t.y);else{if(_e(e,t))return;i.uniform2fv(this.addr,t),ge(e,t)}}function kf(i,t){const e=this.cache;if(t.x!==void 0)(e[0]!==t.x||e[1]!==t.y||e[2]!==t.z)&&(i.uniform3f(this.addr,t.x,t.y,t.z),e[0]=t.x,e[1]=t.y,e[2]=t.z);else if(t.r!==void 0)(e[0]!==t.r||e[1]!==t.g||e[2]!==t.b)&&(i.uniform3f(this.addr,t.r,t.g,t.b),e[0]=t.r,e[1]=t.g,e[2]=t.b);else{if(_e(e,t))return;i.uniform3fv(this.addr,t),ge(e,t)}}function Wf(i,t){const e=this.cache;if(t.x!==void 0)(e[0]!==t.x||e[1]!==t.y||e[2]!==t.z||e[3]!==t.w)&&(i.uniform4f(this.addr,t.x,t.y,t.z,t.w),e[0]=t.x,e[1]=t.y,e[2]=t.z,e[3]=t.w);else{if(_e(e,t))return;i.uniform4fv(this.addr,t),ge(e,t)}}function Xf(i,t){const e=this.cache,n=t.elements;if(n===void 0){if(_e(e,t))return;i.uniformMatrix2fv(this.addr,!1,t),ge(e,t)}else{if(_e(e,n))return;Ho.set(n),i.uniformMatrix2fv(this.addr,!1,Ho),ge(e,n)}}function Yf(i,t){const e=this.cache,n=t.elements;if(n===void 0){if(_e(e,t))return;i.uniformMatrix3fv(this.addr,!1,t),ge(e,t)}else{if(_e(e,n))return;Go.set(n),i.uniformMatrix3fv(this.addr,!1,Go),ge(e,n)}}function qf(i,t){const e=this.cache,n=t.elements;if(n===void 0){if(_e(e,t))return;i.uniformMatrix4fv(this.addr,!1,t),ge(e,t)}else{if(_e(e,n))return;zo.set(n),i.uniformMatrix4fv(this.addr,!1,zo),ge(e,n)}}function Zf(i,t){const e=this.cache;e[0]!==t&&(i.uniform1i(this.addr,t),e[0]=t)}function Kf(i,t){const e=this.cache;if(t.x!==void 0)(e[0]!==t.x||e[1]!==t.y)&&(i.uniform2i(this.addr,t.x,t.y),e[0]=t.x,e[1]=t.y);else{if(_e(e,t))return;i.uniform2iv(this.addr,t),ge(e,t)}}function $f(i,t){const e=this.cache;if(t.x!==void 0)(e[0]!==t.x||e[1]!==t.y||e[2]!==t.z)&&(i.uniform3i(this.addr,t.x,t.y,t.z),e[0]=t.x,e[1]=t.y,e[2]=t.z);else{if(_e(e,t))return;i.uniform3iv(this.addr,t),ge(e,t)}}function Jf(i,t){const e=this.cache;if(t.x!==void 0)(e[0]!==t.x||e[1]!==t.y||e[2]!==t.z||e[3]!==t.w)&&(i.uniform4i(this.addr,t.x,t.y,t.z,t.w),e[0]=t.x,e[1]=t.y,e[2]=t.z,e[3]=t.w);else{if(_e(e,t))return;i.uniform4iv(this.addr,t),ge(e,t)}}function Qf(i,t){const e=this.cache;e[0]!==t&&(i.uniform1ui(this.addr,t),e[0]=t)}function jf(i,t){const e=this.cache;if(t.x!==void 0)(e[0]!==t.x||e[1]!==t.y)&&(i.uniform2ui(this.addr,t.x,t.y),e[0]=t.x,e[1]=t.y);else{if(_e(e,t))return;i.uniform2uiv(this.addr,t),ge(e,t)}}function tp(i,t){const e=this.cache;if(t.x!==void 0)(e[0]!==t.x||e[1]!==t.y||e[2]!==t.z)&&(i.uniform3ui(this.addr,t.x,t.y,t.z),e[0]=t.x,e[1]=t.y,e[2]=t.z);else{if(_e(e,t))return;i.uniform3uiv(this.addr,t),ge(e,t)}}function ep(i,t){const e=this.cache;if(t.x!==void 0)(e[0]!==t.x||e[1]!==t.y||e[2]!==t.z||e[3]!==t.w)&&(i.uniform4ui(this.addr,t.x,t.y,t.z,t.w),e[0]=t.x,e[1]=t.y,e[2]=t.z,e[3]=t.w);else{if(_e(e,t))return;i.uniform4uiv(this.addr,t),ge(e,t)}}function np(i,t,e){const n=this.cache,r=e.allocateTextureUnit();n[0]!==r&&(i.uniform1i(this.addr,r),n[0]=r);let s;this.type===i.SAMPLER_2D_SHADOW?(da.compareFunction=e.isReversedDepthBuffer()?Ea:Ma,s=da):s=Ol,e.setTexture2D(t||s,r)}function ip(i,t,e){const n=this.cache,r=e.allocateTextureUnit();n[0]!==r&&(i.uniform1i(this.addr,r),n[0]=r),e.setTexture3D(t||zl,r)}function rp(i,t,e){const n=this.cache,r=e.allocateTextureUnit();n[0]!==r&&(i.uniform1i(this.addr,r),n[0]=r),e.setTextureCube(t||Gl,r)}function sp(i,t,e){const n=this.cache,r=e.allocateTextureUnit();n[0]!==r&&(i.uniform1i(this.addr,r),n[0]=r),e.setTexture2DArray(t||Bl,r)}function ap(i){switch(i){case 5126:return Hf;case 35664:return Vf;case 35665:return kf;case 35666:return Wf;case 35674:return Xf;case 35675:return Yf;case 35676:return qf;case 5124:case 35670:return Zf;case 35667:case 35671:return Kf;case 35668:case 35672:return $f;case 35669:case 35673:return Jf;case 5125:return Qf;case 36294:return jf;case 36295:return tp;case 36296:return ep;case 35678:case 36198:case 36298:case 36306:case 35682:return np;case 35679:case 36299:case 36307:return ip;case 35680:case 36300:case 36308:case 36293:return rp;case 36289:case 36303:case 36311:case 36292:return sp}}function op(i,t){i.uniform1fv(this.addr,t)}function lp(i,t){const e=Ei(t,this.size,2);i.uniform2fv(this.addr,e)}function cp(i,t){const e=Ei(t,this.size,3);i.uniform3fv(this.addr,e)}function hp(i,t){const e=Ei(t,this.size,4);i.uniform4fv(this.addr,e)}function up(i,t){const e=Ei(t,this.size,4);i.uniformMatrix2fv(this.addr,!1,e)}function dp(i,t){const e=Ei(t,this.size,9);i.uniformMatrix3fv(this.addr,!1,e)}function fp(i,t){const e=Ei(t,this.size,16);i.uniformMatrix4fv(this.addr,!1,e)}function pp(i,t){i.uniform1iv(this.addr,t)}function mp(i,t){i.uniform2iv(this.addr,t)}function _p(i,t){i.uniform3iv(this.addr,t)}function gp(i,t){i.uniform4iv(this.addr,t)}function xp(i,t){i.uniform1uiv(this.addr,t)}function vp(i,t){i.uniform2uiv(this.addr,t)}function Sp(i,t){i.uniform3uiv(this.addr,t)}function Mp(i,t){i.uniform4uiv(this.addr,t)}function Ep(i,t,e){const n=this.cache,r=t.length,s=Hr(e,r);_e(n,s)||(i.uniform1iv(this.addr,s),ge(n,s));let a;this.type===i.SAMPLER_2D_SHADOW?a=da:a=Ol;for(let o=0;o!==r;++o)e.setTexture2D(t[o]||a,s[o])}function yp(i,t,e){const n=this.cache,r=t.length,s=Hr(e,r);_e(n,s)||(i.uniform1iv(this.addr,s),ge(n,s));for(let a=0;a!==r;++a)e.setTexture3D(t[a]||zl,s[a])}function bp(i,t,e){const n=this.cache,r=t.length,s=Hr(e,r);_e(n,s)||(i.uniform1iv(this.addr,s),ge(n,s));for(let a=0;a!==r;++a)e.setTextureCube(t[a]||Gl,s[a])}function Tp(i,t,e){const n=this.cache,r=t.length,s=Hr(e,r);_e(n,s)||(i.uniform1iv(this.addr,s),ge(n,s));for(let a=0;a!==r;++a)e.setTexture2DArray(t[a]||Bl,s[a])}function Ap(i){switch(i){case 5126:return op;case 35664:return lp;case 35665:return cp;case 35666:return hp;case 35674:return up;case 35675:return dp;case 35676:return fp;case 5124:case 35670:return pp;case 35667:case 35671:return mp;case 35668:case 35672:return _p;case 35669:case 35673:return gp;case 5125:return xp;case 36294:return vp;case 36295:return Sp;case 36296:return Mp;case 35678:case 36198:case 36298:case 36306:case 35682:return Ep;case 35679:case 36299:case 36307:return yp;case 35680:case 36300:case 36308:case 36293:return bp;case 36289:case 36303:case 36311:case 36292:return Tp}}class wp{constructor(t,e,n){this.id=t,this.addr=n,this.cache=[],this.type=e.type,this.setValue=ap(e.type)}}class Rp{constructor(t,e,n){this.id=t,this.addr=n,this.cache=[],this.type=e.type,this.size=e.size,this.setValue=Ap(e.type)}}class Cp{constructor(t){this.id=t,this.seq=[],this.map={}}setValue(t,e,n){const r=this.seq;for(let s=0,a=r.length;s!==a;++s){const o=r[s];o.setValue(t,e[o.id],n)}}}const Es=/(\w+)(\])?(\[|\.)?/g;function Vo(i,t){i.seq.push(t),i.map[t.id]=t}function Pp(i,t,e){const n=i.name,r=n.length;for(Es.lastIndex=0;;){const s=Es.exec(n),a=Es.lastIndex;let o=s[1];const c=s[2]==="]",l=s[3];if(c&&(o=o|0),l===void 0||l==="["&&a+2===r){Vo(e,l===void 0?new wp(o,i,t):new Rp(o,i,t));break}else{let p=e.map[o];p===void 0&&(p=new Cp(o),Vo(e,p)),e=p}}}class wr{constructor(t,e){this.seq=[],this.map={};const n=t.getProgramParameter(e,t.ACTIVE_UNIFORMS);for(let a=0;a<n;++a){const o=t.getActiveUniform(e,a),c=t.getUniformLocation(e,o.name);Pp(o,c,this)}const r=[],s=[];for(const a of this.seq)a.type===t.SAMPLER_2D_SHADOW||a.type===t.SAMPLER_CUBE_SHADOW||a.type===t.SAMPLER_2D_ARRAY_SHADOW?r.push(a):s.push(a);r.length>0&&(this.seq=r.concat(s))}setValue(t,e,n,r){const s=this.map[e];s!==void 0&&s.setValue(t,n,r)}setOptional(t,e,n){const r=e[n];r!==void 0&&this.setValue(t,n,r)}static upload(t,e,n,r){for(let s=0,a=e.length;s!==a;++s){const o=e[s],c=n[o.id];c.needsUpdate!==!1&&o.setValue(t,c.value,r)}}static seqWithValue(t,e){const n=[];for(let r=0,s=t.length;r!==s;++r){const a=t[r];a.id in e&&n.push(a)}return n}}function ko(i,t,e){const n=i.createShader(t);return i.shaderSource(n,e),i.compileShader(n),n}const Dp=37297;let Lp=0;function Ip(i,t){const e=i.split(`
`),n=[],r=Math.max(t-6,0),s=Math.min(t+6,e.length);for(let a=r;a<s;a++){const o=a+1;n.push(`${o===t?">":" "} ${o}: ${e[a]}`)}return n.join(`
`)}const Wo=new Dt;function Up(i){Ht._getMatrix(Wo,Ht.workingColorSpace,i);const t=`mat3( ${Wo.elements.map(e=>e.toFixed(4))} )`;switch(Ht.getTransfer(i)){case Dr:return[t,"LinearTransferOETF"];case $t:return[t,"sRGBTransferOETF"];default:return Rt("WebGLProgram: Unsupported color space: ",i),[t,"LinearTransferOETF"]}}function Xo(i,t,e){const n=i.getShaderParameter(t,i.COMPILE_STATUS),s=(i.getShaderInfoLog(t)||"").trim();if(n&&s==="")return"";const a=/ERROR: 0:(\d+)/.exec(s);if(a){const o=parseInt(a[1]);return e.toUpperCase()+`

`+s+`

`+Ip(i.getShaderSource(t),o)}else return s}function Np(i,t){const e=Up(t);return[`vec4 ${i}( vec4 value ) {`,`	return ${e[1]}( vec4( value.rgb * ${e[0]}, value.a ) );`,"}"].join(`
`)}const Fp={[sl]:"Linear",[al]:"Reinhard",[ol]:"Cineon",[ll]:"ACESFilmic",[hl]:"AgX",[ul]:"Neutral",[cl]:"Custom"};function Op(i,t){const e=Fp[t];return e===void 0?(Rt("WebGLProgram: Unsupported toneMapping:",t),"vec3 "+i+"( vec3 color ) { return LinearToneMapping( color ); }"):"vec3 "+i+"( vec3 color ) { return "+e+"ToneMapping( color ); }"}const Sr=new I;function Bp(){Ht.getLuminanceCoefficients(Sr);const i=Sr.x.toFixed(4),t=Sr.y.toFixed(4),e=Sr.z.toFixed(4);return["float luminance( const in vec3 rgb ) {",`	const vec3 weights = vec3( ${i}, ${t}, ${e} );`,"	return dot( weights, rgb );","}"].join(`
`)}function zp(i){return[i.extensionClipCullDistance?"#extension GL_ANGLE_clip_cull_distance : require":"",i.extensionMultiDraw?"#extension GL_ANGLE_multi_draw : require":""].filter(Ii).join(`
`)}function Gp(i){const t=[];for(const e in i){const n=i[e];n!==!1&&t.push("#define "+e+" "+n)}return t.join(`
`)}function Hp(i,t){const e={},n=i.getProgramParameter(t,i.ACTIVE_ATTRIBUTES);for(let r=0;r<n;r++){const s=i.getActiveAttrib(t,r),a=s.name;let o=1;s.type===i.FLOAT_MAT2&&(o=2),s.type===i.FLOAT_MAT3&&(o=3),s.type===i.FLOAT_MAT4&&(o=4),e[a]={type:s.type,location:i.getAttribLocation(t,a),locationSize:o}}return e}function Ii(i){return i!==""}function Yo(i,t){const e=t.numSpotLightShadows+t.numSpotLightMaps-t.numSpotLightShadowsWithMaps;return i.replace(/NUM_SUN_LIGHTS/g,t.numSunLights).replace(/NUM_DIR_LIGHTS/g,t.numDirLights).replace(/NUM_SPOT_LIGHTS/g,t.numSpotLights).replace(/NUM_SPOT_LIGHT_MAPS/g,t.numSpotLightMaps).replace(/NUM_SPOT_LIGHT_COORDS/g,e).replace(/NUM_RECT_AREA_LIGHTS/g,t.numRectAreaLights).replace(/NUM_POINT_LIGHTS/g,t.numPointLights).replace(/NUM_HEMI_LIGHTS/g,t.numHemiLights).replace(/NUM_SUN_LIGHT_SHADOWS/g,t.numSunLightShadows).replace(/NUM_DIR_LIGHT_SHADOWS/g,t.numDirLightShadows).replace(/NUM_SPOT_LIGHT_SHADOWS_WITH_MAPS/g,t.numSpotLightShadowsWithMaps).replace(/NUM_SPOT_LIGHT_SHADOWS/g,t.numSpotLightShadows).replace(/NUM_POINT_LIGHT_SHADOWS/g,t.numPointLightShadows)}function qo(i,t){return i.replace(/NUM_CLIPPING_PLANES/g,t.numClippingPlanes).replace(/UNION_CLIPPING_PLANES/g,t.numClippingPlanes-t.numClipIntersection)}const Vp=/^[ \t]*#include +<([\w\d./]+)>/gm;function fa(i){return i.replace(Vp,Wp)}const kp=new Map;function Wp(i,t){let e=Nt[t];if(e===void 0){const n=kp.get(t);if(n!==void 0)e=Nt[n],Rt('WebGLRenderer: Shader chunk "%s" has been deprecated. Use "%s" instead.',t,n);else throw new Error("THREE.WebGLProgram: Can not resolve #include <"+t+">")}return fa(e)}const Xp=/#pragma unroll_loop_start\s+for\s*\(\s*int\s+i\s*=\s*(\d+)\s*;\s*i\s*<\s*(\d+)\s*;\s*i\s*\+\+\s*\)\s*{([\s\S]+?)}\s+#pragma unroll_loop_end/g;function Zo(i){return i.replace(Xp,Yp)}function Yp(i,t,e,n){let r="";for(let s=parseInt(t);s<parseInt(e);s++)r+=n.replace(/\[\s*i\s*\]/g,"[ "+s+" ]").replace(/UNROLLED_LOOP_INDEX/g,s);return r}function Ko(i){let t=`precision ${i.precision} float;
	precision ${i.precision} int;
	precision ${i.precision} sampler2D;
	precision ${i.precision} samplerCube;
	precision ${i.precision} sampler3D;
	precision ${i.precision} sampler2DArray;
	precision ${i.precision} sampler2DShadow;
	precision ${i.precision} samplerCubeShadow;
	precision ${i.precision} sampler2DArrayShadow;
	precision ${i.precision} isampler2D;
	precision ${i.precision} isampler3D;
	precision ${i.precision} isamplerCube;
	precision ${i.precision} isampler2DArray;
	precision ${i.precision} usampler2D;
	precision ${i.precision} usampler3D;
	precision ${i.precision} usamplerCube;
	precision ${i.precision} usampler2DArray;
	`;return i.precision==="highp"?t+=`
#define HIGH_PRECISION`:i.precision==="mediump"?t+=`
#define MEDIUM_PRECISION`:i.precision==="lowp"&&(t+=`
#define LOW_PRECISION`),t}const qp={[Er]:"SHADOWMAP_TYPE_PCF",[Li]:"SHADOWMAP_TYPE_VSM"};function Zp(i){return qp[i.shadowMapType]||"SHADOWMAP_TYPE_BASIC"}const Kp={[Wn]:"ENVMAP_TYPE_CUBE",[xi]:"ENVMAP_TYPE_CUBE",[Or]:"ENVMAP_TYPE_CUBE_UV"};function $p(i){return i.envMap===!1?"ENVMAP_TYPE_CUBE":Kp[i.envMapMode]||"ENVMAP_TYPE_CUBE"}const Jp={[xi]:"ENVMAP_MODE_REFRACTION"};function Qp(i){return i.envMap===!1?"ENVMAP_MODE_REFLECTION":Jp[i.envMapMode]||"ENVMAP_MODE_REFLECTION"}const jp={[rl]:"ENVMAP_BLENDING_MULTIPLY",[gc]:"ENVMAP_BLENDING_MIX",[xc]:"ENVMAP_BLENDING_ADD"};function tm(i){return i.envMap===!1?"ENVMAP_BLENDING_NONE":jp[i.combine]||"ENVMAP_BLENDING_NONE"}function em(i){const t=i.envMapCubeUVHeight;if(t===null)return null;const e=Math.log2(t)-2,n=1/t;return{texelWidth:1/(3*Math.max(Math.pow(2,e),112)),texelHeight:n,maxMip:e}}function nm(i,t,e,n){const r=i.getContext(),s=e.defines;let a=e.vertexShader,o=e.fragmentShader;const c=Zp(e),l=$p(e),d=Qp(e),p=tm(e),h=em(e),_=zp(e),v=Gp(s),M=r.createProgram();let m,u,A=e.glslVersion?"#version "+e.glslVersion+`
`:"";e.isRawShaderMaterial?(m=["#define SHADER_TYPE "+e.shaderType,"#define SHADER_NAME "+e.shaderName,v].filter(Ii).join(`
`),m.length>0&&(m+=`
`),u=["#define SHADER_TYPE "+e.shaderType,"#define SHADER_NAME "+e.shaderName,v].filter(Ii).join(`
`),u.length>0&&(u+=`
`)):(m=[Ko(e),"#define SHADER_TYPE "+e.shaderType,"#define SHADER_NAME "+e.shaderName,v,e.extensionClipCullDistance?"#define USE_CLIP_DISTANCE":"",e.batching?"#define USE_BATCHING":"",e.batchingColor?"#define USE_BATCHING_COLOR":"",e.instancing?"#define USE_INSTANCING":"",e.instancingColor?"#define USE_INSTANCING_COLOR":"",e.instancingMorph?"#define USE_INSTANCING_MORPH":"",e.useFog&&e.fog?"#define USE_FOG":"",e.useFog&&e.fogExp2?"#define FOG_EXP2":"",e.map?"#define USE_MAP":"",e.envMap?"#define USE_ENVMAP":"",e.envMap?"#define "+d:"",e.lightMap?"#define USE_LIGHTMAP":"",e.aoMap?"#define USE_AOMAP":"",e.bumpMap?"#define USE_BUMPMAP":"",e.normalMap?"#define USE_NORMALMAP":"",e.normalMapObjectSpace?"#define USE_NORMALMAP_OBJECTSPACE":"",e.normalMapTangentSpace?"#define USE_NORMALMAP_TANGENTSPACE":"",e.displacementMap?"#define USE_DISPLACEMENTMAP":"",e.emissiveMap?"#define USE_EMISSIVEMAP":"",e.anisotropy?"#define USE_ANISOTROPY":"",e.anisotropyMap?"#define USE_ANISOTROPYMAP":"",e.clearcoatMap?"#define USE_CLEARCOATMAP":"",e.clearcoatRoughnessMap?"#define USE_CLEARCOAT_ROUGHNESSMAP":"",e.clearcoatNormalMap?"#define USE_CLEARCOAT_NORMALMAP":"",e.iridescenceMap?"#define USE_IRIDESCENCEMAP":"",e.iridescenceThicknessMap?"#define USE_IRIDESCENCE_THICKNESSMAP":"",e.specularMap?"#define USE_SPECULARMAP":"",e.specularColorMap?"#define USE_SPECULAR_COLORMAP":"",e.specularIntensityMap?"#define USE_SPECULAR_INTENSITYMAP":"",e.roughnessMap?"#define USE_ROUGHNESSMAP":"",e.metalnessMap?"#define USE_METALNESSMAP":"",e.alphaMap?"#define USE_ALPHAMAP":"",e.alphaHash?"#define USE_ALPHAHASH":"",e.transmission?"#define USE_TRANSMISSION":"",e.transmissionMap?"#define USE_TRANSMISSIONMAP":"",e.thicknessMap?"#define USE_THICKNESSMAP":"",e.sheenColorMap?"#define USE_SHEEN_COLORMAP":"",e.sheenRoughnessMap?"#define USE_SHEEN_ROUGHNESSMAP":"",e.mapUv?"#define MAP_UV "+e.mapUv:"",e.alphaMapUv?"#define ALPHAMAP_UV "+e.alphaMapUv:"",e.lightMapUv?"#define LIGHTMAP_UV "+e.lightMapUv:"",e.aoMapUv?"#define AOMAP_UV "+e.aoMapUv:"",e.emissiveMapUv?"#define EMISSIVEMAP_UV "+e.emissiveMapUv:"",e.bumpMapUv?"#define BUMPMAP_UV "+e.bumpMapUv:"",e.normalMapUv?"#define NORMALMAP_UV "+e.normalMapUv:"",e.displacementMapUv?"#define DISPLACEMENTMAP_UV "+e.displacementMapUv:"",e.metalnessMapUv?"#define METALNESSMAP_UV "+e.metalnessMapUv:"",e.roughnessMapUv?"#define ROUGHNESSMAP_UV "+e.roughnessMapUv:"",e.anisotropyMapUv?"#define ANISOTROPYMAP_UV "+e.anisotropyMapUv:"",e.clearcoatMapUv?"#define CLEARCOATMAP_UV "+e.clearcoatMapUv:"",e.clearcoatNormalMapUv?"#define CLEARCOAT_NORMALMAP_UV "+e.clearcoatNormalMapUv:"",e.clearcoatRoughnessMapUv?"#define CLEARCOAT_ROUGHNESSMAP_UV "+e.clearcoatRoughnessMapUv:"",e.iridescenceMapUv?"#define IRIDESCENCEMAP_UV "+e.iridescenceMapUv:"",e.iridescenceThicknessMapUv?"#define IRIDESCENCE_THICKNESSMAP_UV "+e.iridescenceThicknessMapUv:"",e.sheenColorMapUv?"#define SHEEN_COLORMAP_UV "+e.sheenColorMapUv:"",e.sheenRoughnessMapUv?"#define SHEEN_ROUGHNESSMAP_UV "+e.sheenRoughnessMapUv:"",e.specularMapUv?"#define SPECULARMAP_UV "+e.specularMapUv:"",e.specularColorMapUv?"#define SPECULAR_COLORMAP_UV "+e.specularColorMapUv:"",e.specularIntensityMapUv?"#define SPECULAR_INTENSITYMAP_UV "+e.specularIntensityMapUv:"",e.transmissionMapUv?"#define TRANSMISSIONMAP_UV "+e.transmissionMapUv:"",e.thicknessMapUv?"#define THICKNESSMAP_UV "+e.thicknessMapUv:"",e.vertexTangents&&e.flatShading===!1?"#define USE_TANGENT":"",e.vertexNormals?"#define HAS_NORMAL":"",e.vertexColors?"#define USE_COLOR":"",e.vertexAlphas?"#define USE_COLOR_ALPHA":"",e.vertexUv1s?"#define USE_UV1":"",e.vertexUv2s?"#define USE_UV2":"",e.vertexUv3s?"#define USE_UV3":"",e.pointsUvs?"#define USE_POINTS_UV":"",e.flatShading?"#define FLAT_SHADED":"",e.skinning?"#define USE_SKINNING":"",e.morphTargets?"#define USE_MORPHTARGETS":"",e.morphNormals&&e.flatShading===!1?"#define USE_MORPHNORMALS":"",e.morphColors?"#define USE_MORPHCOLORS":"",e.morphTargetsCount>0?"#define MORPHTARGETS_TEXTURE_STRIDE "+e.morphTextureStride:"",e.morphTargetsCount>0?"#define MORPHTARGETS_COUNT "+e.morphTargetsCount:"",e.doubleSided?"#define DOUBLE_SIDED":"",e.flipSided?"#define FLIP_SIDED":"",e.shadowMapEnabled?"#define USE_SHADOWMAP":"",e.shadowMapEnabled?"#define "+c:"",e.sizeAttenuation?"#define USE_SIZEATTENUATION":"",e.numLightProbes>0?"#define USE_LIGHT_PROBES":"",e.logarithmicDepthBuffer?"#define USE_LOGARITHMIC_DEPTH_BUFFER":"",e.reversedDepthBuffer?"#define USE_REVERSED_DEPTH_BUFFER":"","uniform mat4 modelMatrix;","uniform mat4 modelViewMatrix;","uniform mat4 projectionMatrix;","uniform mat4 viewMatrix;","uniform mat3 normalMatrix;","uniform vec3 cameraPosition;","uniform bool isOrthographic;","#ifdef USE_INSTANCING","	attribute mat4 instanceMatrix;","#endif","#ifdef USE_INSTANCING_COLOR","	attribute vec3 instanceColor;","#endif","#ifdef USE_INSTANCING_MORPH","	uniform sampler2D morphTexture;","#endif","attribute vec3 position;","attribute vec3 normal;","attribute vec2 uv;","#ifdef USE_UV1","	attribute vec2 uv1;","#endif","#ifdef USE_UV2","	attribute vec2 uv2;","#endif","#ifdef USE_UV3","	attribute vec2 uv3;","#endif","#ifdef USE_TANGENT","	attribute vec4 tangent;","#endif","#if defined( USE_COLOR_ALPHA )","	attribute vec4 color;","#elif defined( USE_COLOR )","	attribute vec3 color;","#endif","#ifdef USE_SKINNING","	attribute vec4 skinIndex;","	attribute vec4 skinWeight;","#endif",`
`].filter(Ii).join(`
`),u=[Ko(e),"#define SHADER_TYPE "+e.shaderType,"#define SHADER_NAME "+e.shaderName,v,e.useFog&&e.fog?"#define USE_FOG":"",e.useFog&&e.fogExp2?"#define FOG_EXP2":"",e.alphaToCoverage?"#define ALPHA_TO_COVERAGE":"",e.map?"#define USE_MAP":"",e.matcap?"#define USE_MATCAP":"",e.envMap?"#define USE_ENVMAP":"",e.envMap?"#define "+l:"",e.envMap?"#define "+d:"",e.envMap?"#define "+p:"",h?"#define CUBEUV_TEXEL_WIDTH "+h.texelWidth:"",h?"#define CUBEUV_TEXEL_HEIGHT "+h.texelHeight:"",h?"#define CUBEUV_MAX_MIP "+h.maxMip+".0":"",e.lightMap?"#define USE_LIGHTMAP":"",e.aoMap?"#define USE_AOMAP":"",e.bumpMap?"#define USE_BUMPMAP":"",e.normalMap?"#define USE_NORMALMAP":"",e.normalMapObjectSpace?"#define USE_NORMALMAP_OBJECTSPACE":"",e.normalMapTangentSpace?"#define USE_NORMALMAP_TANGENTSPACE":"",e.packedNormalMap?"#define USE_PACKED_NORMALMAP":"",e.emissiveMap?"#define USE_EMISSIVEMAP":"",e.anisotropy?"#define USE_ANISOTROPY":"",e.anisotropyMap?"#define USE_ANISOTROPYMAP":"",e.clearcoat?"#define USE_CLEARCOAT":"",e.clearcoatMap?"#define USE_CLEARCOATMAP":"",e.clearcoatRoughnessMap?"#define USE_CLEARCOAT_ROUGHNESSMAP":"",e.clearcoatNormalMap?"#define USE_CLEARCOAT_NORMALMAP":"",e.dispersion?"#define USE_DISPERSION":"",e.retroreflection?"#define USE_RETROREFLECTION":"",e.iridescence?"#define USE_IRIDESCENCE":"",e.iridescenceMap?"#define USE_IRIDESCENCEMAP":"",e.iridescenceThicknessMap?"#define USE_IRIDESCENCE_THICKNESSMAP":"",e.specularMap?"#define USE_SPECULARMAP":"",e.specularColorMap?"#define USE_SPECULAR_COLORMAP":"",e.specularIntensityMap?"#define USE_SPECULAR_INTENSITYMAP":"",e.roughnessMap?"#define USE_ROUGHNESSMAP":"",e.metalnessMap?"#define USE_METALNESSMAP":"",e.alphaMap?"#define USE_ALPHAMAP":"",e.alphaTest?"#define USE_ALPHATEST":"",e.alphaHash?"#define USE_ALPHAHASH":"",e.sheen?"#define USE_SHEEN":"",e.sheenColorMap?"#define USE_SHEEN_COLORMAP":"",e.sheenRoughnessMap?"#define USE_SHEEN_ROUGHNESSMAP":"",e.transmission?"#define USE_TRANSMISSION":"",e.transmissionMap?"#define USE_TRANSMISSIONMAP":"",e.thicknessMap?"#define USE_THICKNESSMAP":"",e.vertexTangents&&e.flatShading===!1?"#define USE_TANGENT":"",e.vertexColors||e.instancingColor?"#define USE_COLOR":"",e.vertexAlphas||e.batchingColor?"#define USE_COLOR_ALPHA":"",e.vertexUv1s?"#define USE_UV1":"",e.vertexUv2s?"#define USE_UV2":"",e.vertexUv3s?"#define USE_UV3":"",e.pointsUvs?"#define USE_POINTS_UV":"",e.gradientMap?"#define USE_GRADIENTMAP":"",e.flatShading?"#define FLAT_SHADED":"",e.doubleSided?"#define DOUBLE_SIDED":"",e.flipSided?"#define FLIP_SIDED":"",e.shadowMapEnabled?"#define USE_SHADOWMAP":"",e.shadowMapEnabled?"#define "+c:"",e.premultipliedAlpha?"#define PREMULTIPLIED_ALPHA":"",e.numLightProbes>0?"#define USE_LIGHT_PROBES":"",e.numLightProbeGrids>0?"#define USE_LIGHT_PROBES_GRID":"",e.decodeVideoTexture?"#define DECODE_VIDEO_TEXTURE":"",e.decodeVideoTextureEmissive?"#define DECODE_VIDEO_TEXTURE_EMISSIVE":"",e.logarithmicDepthBuffer?"#define USE_LOGARITHMIC_DEPTH_BUFFER":"",e.reversedDepthBuffer?"#define USE_REVERSED_DEPTH_BUFFER":"","uniform mat4 viewMatrix;","uniform vec3 cameraPosition;","uniform bool isOrthographic;",e.toneMapping!==on?"#define TONE_MAPPING":"",e.toneMapping!==on?Nt.tonemapping_pars_fragment:"",e.toneMapping!==on?Op("toneMapping",e.toneMapping):"",e.dithering?"#define DITHERING":"",e.opaque?"#define OPAQUE":"",Nt.colorspace_pars_fragment,Np("linearToOutputTexel",e.outputColorSpace),Bp(),e.useDepthPacking?"#define DEPTH_PACKING "+e.depthPacking:"",`
`].filter(Ii).join(`
`)),a=fa(a),a=Yo(a,e),a=qo(a,e),o=fa(o),o=Yo(o,e),o=qo(o,e),a=Zo(a),o=Zo(o),e.isRawShaderMaterial!==!0&&(A=`#version 300 es
`,m=[_,"#define attribute in","#define varying out","#define texture2D texture"].join(`
`)+`
`+m,u=["#define varying in",e.glslVersion===Ka?"":"layout(location = 0) out highp vec4 pc_fragColor;",e.glslVersion===Ka?"":"#define gl_FragColor pc_fragColor","#define gl_FragDepthEXT gl_FragDepth","#define texture2D texture","#define textureCube texture","#define texture2DProj textureProj","#define texture2DLodEXT textureLod","#define texture2DProjLodEXT textureProjLod","#define textureCubeLodEXT textureLod","#define texture2DGradEXT textureGrad","#define texture2DProjGradEXT textureProjGrad","#define textureCubeGradEXT textureGrad"].join(`
`)+`
`+u);const P=A+m+a,S=A+u+o,b=ko(r,r.VERTEX_SHADER,P),y=ko(r,r.FRAGMENT_SHADER,S);r.attachShader(M,b),r.attachShader(M,y),e.index0AttributeName!==void 0?r.bindAttribLocation(M,0,e.index0AttributeName):e.hasPositionAttribute===!0&&r.bindAttribLocation(M,0,"position"),r.linkProgram(M);function w(F){if(i.debug.checkShaderErrors){const G=r.getProgramInfoLog(M)||"",k=r.getShaderInfoLog(b)||"",U=r.getShaderInfoLog(y)||"",H=G.trim(),K=k.trim(),Z=U.trim();let nt=!0,X=!0;if(r.getProgramParameter(M,r.LINK_STATUS)===!1)if(nt=!1,typeof i.debug.onShaderError=="function")i.debug.onShaderError(r,M,b,y);else{const j=Xo(r,b,"vertex"),et=Xo(r,y,"fragment");Wt("WebGLProgram: Shader Error "+r.getError()+" - VALIDATE_STATUS "+r.getProgramParameter(M,r.VALIDATE_STATUS)+`

Material Name: `+F.name+`
Material Type: `+F.type+`

Program Info Log: `+H+`
`+j+`
`+et)}else H!==""?Rt("WebGLProgram: Program Info Log:",H):(K===""||Z==="")&&(X=!1);X&&(F.diagnostics={runnable:nt,programLog:H,vertexShader:{log:K,prefix:m},fragmentShader:{log:Z,prefix:u}})}r.deleteShader(b),r.deleteShader(y),x=new wr(r,M),T=Hp(r,M)}let x;this.getUniforms=function(){return x===void 0&&w(this),x};let T;this.getAttributes=function(){return T===void 0&&w(this),T};let L=e.rendererExtensionParallelShaderCompile===!1;return this.isReady=function(){return L===!1&&(L=r.getProgramParameter(M,Dp)),L},this.destroy=function(){n.releaseStatesOfProgram(this),r.deleteProgram(M),this.program=void 0},this.type=e.shaderType,this.name=e.shaderName,this.id=Lp++,this.cacheKey=t,this.usedTimes=1,this.program=M,this.vertexShader=b,this.fragmentShader=y,this}let im=0;class rm{constructor(){this.shaderCache=new Map,this.materialCache=new Map}update(t,e,n){const r=this._getShaderCacheForMaterial(t);return r.has(e)===!1&&(r.add(e),e.usedTimes++),r.has(n)===!1&&(r.add(n),n.usedTimes++),this}remove(t){const e=this.materialCache.get(t);for(const n of e)n.usedTimes--,n.usedTimes===0&&this.shaderCache.delete(n.code);return this.materialCache.delete(t),this}getVertexShaderStage(t){return this._getShaderStage(t.vertexShader)}getFragmentShaderStage(t){return this._getShaderStage(t.fragmentShader)}dispose(){this.shaderCache.clear(),this.materialCache.clear()}_getShaderCacheForMaterial(t){const e=this.materialCache;let n=e.get(t);return n===void 0&&(n=new Set,e.set(t,n)),n}_getShaderStage(t){const e=this.shaderCache;let n=e.get(t);return n===void 0&&(n=new sm(t),e.set(t,n)),n}}class sm{constructor(t){this.id=im++,this.code=t,this.usedTimes=0}}function am(i){return i===Xn||i===Rr||i===Cr}function om(i,t,e,n,r,s){const a=new Ta,o=new rm,c=new Set,l=[],d=new Map,p=n.logarithmicDepthBuffer;let h=n.precision;const _={MeshDepthMaterial:"depth",MeshDistanceMaterial:"distance",MeshNormalMaterial:"normal",MeshBasicMaterial:"basic",MeshLambertMaterial:"lambert",MeshPhongMaterial:"phong",MeshToonMaterial:"toon",MeshStandardMaterial:"physical",MeshPhysicalMaterial:"physical",MeshMatcapMaterial:"matcap",LineBasicMaterial:"basic",LineDashedMaterial:"dashed",PointsMaterial:"points",ShadowMaterial:"shadow",SpriteMaterial:"sprite"};function v(x){return c.add(x),x===0?"uv":`uv${x}`}function M(x,T,L,F,G,k){const U=F.fog,H=G.geometry,K=x.isMeshStandardMaterial||x.isMeshLambertMaterial||x.isMeshPhongMaterial?F.environment:null,Z=x.isMeshStandardMaterial||x.isMeshLambertMaterial&&!x.envMap||x.isMeshPhongMaterial&&!x.envMap,nt=t.get(x.envMap||K,Z),X=nt&&nt.mapping===Or?nt.image.height:null,j=_[x.type];x.precision!==null&&(h=n.getMaxPrecision(x.precision),h!==x.precision&&Rt("WebGLProgram.getParameters:",x.precision,"not supported, using",h,"instead."));const et=H.morphAttributes.position||H.morphAttributes.normal||H.morphAttributes.color,At=et!==void 0?et.length:0;let bt=0;H.morphAttributes.position!==void 0&&(bt=1),H.morphAttributes.normal!==void 0&&(bt=2),H.morphAttributes.color!==void 0&&(bt=3);let ne,Vt,Yt,Y;if(j){const re=sn[j];ne=re.vertexShader,Vt=re.fragmentShader}else{ne=x.vertexShader,Vt=x.fragmentShader;const re=o.getVertexShaderStage(x),qt=o.getFragmentShaderStage(x);o.update(x,re,qt),Yt=re.id,Y=qt.id}const Q=i.getRenderTarget(),gt=i.state.buffers.depth.getReversed(),Pt=G.isInstancedMesh===!0,mt=G.isBatchedMesh===!0,Ft=!!x.map,pe=!!x.matcap,Ot=!!nt,Xt=!!x.aoMap,ie=!!x.lightMap,zt=!!x.bumpMap&&x.wireframe===!1,oe=!!x.normalMap,xe=!!x.displacementMap,Le=!!x.emissiveMap,le=!!x.metalnessMap,ue=!!x.roughnessMap,D=x.anisotropy>0,Ee=x.clearcoat>0,Kt=x.dispersion>0,E=x.retroreflectivity>0,f=x.iridescence>0,N=x.sheen>0,z=x.transmission>0,W=D&&!!x.anisotropyMap,it=Ee&&!!x.clearcoatMap,rt=Ee&&!!x.clearcoatNormalMap,q=Ee&&!!x.clearcoatRoughnessMap,J=f&&!!x.iridescenceMap,st=f&&!!x.iridescenceThicknessMap,Et=N&&!!x.sheenColorMap,ct=N&&!!x.sheenRoughnessMap,at=!!x.specularMap,yt=!!x.specularColorMap,wt=!!x.specularIntensityMap,It=z&&!!x.transmissionMap,C=z&&!!x.thicknessMap,ot=!!x.gradientMap,$=!!x.alphaMap,lt=x.alphaTest>0,ft=!!x.alphaHash,tt=!!x.extensions;let Tt=on;x.toneMapped&&(Q===null||Q.isXRRenderTarget===!0)&&(Tt=i.toneMapping);const St={shaderID:j,shaderType:x.type,shaderName:x.name,vertexShader:ne,fragmentShader:Vt,defines:x.defines,customVertexShaderID:Yt,customFragmentShaderID:Y,isRawShaderMaterial:x.isRawShaderMaterial===!0,glslVersion:x.glslVersion,precision:h,batching:mt,batchingColor:mt&&G._colorsTexture!==null,instancing:Pt,instancingColor:Pt&&G.instanceColor!==null,instancingMorph:Pt&&G.morphTexture!==null,outputColorSpace:Q===null?i.outputColorSpace:Q.isXRRenderTarget===!0?Q.texture.colorSpace:Ht.workingColorSpace,alphaToCoverage:!!x.alphaToCoverage,map:Ft,matcap:pe,envMap:Ot,envMapMode:Ot&&nt.mapping,envMapCubeUVHeight:X,aoMap:Xt,lightMap:ie,bumpMap:zt,normalMap:oe,displacementMap:xe,emissiveMap:Le,normalMapObjectSpace:oe&&x.normalMapType===Mc,normalMapTangentSpace:oe&&x.normalMapType===ha,packedNormalMap:oe&&x.normalMapType===ha&&am(x.normalMap.format),metalnessMap:le,roughnessMap:ue,anisotropy:D,anisotropyMap:W,clearcoat:Ee,clearcoatMap:it,clearcoatNormalMap:rt,clearcoatRoughnessMap:q,dispersion:Kt,retroreflection:E,iridescence:f,iridescenceMap:J,iridescenceThicknessMap:st,sheen:N,sheenColorMap:Et,sheenRoughnessMap:ct,specularMap:at,specularColorMap:yt,specularIntensityMap:wt,transmission:z,transmissionMap:It,thicknessMap:C,gradientMap:ot,opaque:x.transparent===!1&&x.blending===Ui&&x.alphaToCoverage===!1,alphaMap:$,alphaTest:lt,alphaHash:ft,combine:x.combine,mapUv:Ft&&v(x.map.channel),aoMapUv:Xt&&v(x.aoMap.channel),lightMapUv:ie&&v(x.lightMap.channel),bumpMapUv:zt&&v(x.bumpMap.channel),normalMapUv:oe&&v(x.normalMap.channel),displacementMapUv:xe&&v(x.displacementMap.channel),emissiveMapUv:Le&&v(x.emissiveMap.channel),metalnessMapUv:le&&v(x.metalnessMap.channel),roughnessMapUv:ue&&v(x.roughnessMap.channel),anisotropyMapUv:W&&v(x.anisotropyMap.channel),clearcoatMapUv:it&&v(x.clearcoatMap.channel),clearcoatNormalMapUv:rt&&v(x.clearcoatNormalMap.channel),clearcoatRoughnessMapUv:q&&v(x.clearcoatRoughnessMap.channel),iridescenceMapUv:J&&v(x.iridescenceMap.channel),iridescenceThicknessMapUv:st&&v(x.iridescenceThicknessMap.channel),sheenColorMapUv:Et&&v(x.sheenColorMap.channel),sheenRoughnessMapUv:ct&&v(x.sheenRoughnessMap.channel),specularMapUv:at&&v(x.specularMap.channel),specularColorMapUv:yt&&v(x.specularColorMap.channel),specularIntensityMapUv:wt&&v(x.specularIntensityMap.channel),transmissionMapUv:It&&v(x.transmissionMap.channel),thicknessMapUv:C&&v(x.thicknessMap.channel),alphaMapUv:$&&v(x.alphaMap.channel),vertexTangents:!!H.attributes.tangent&&(oe||D),vertexNormals:!!H.attributes.normal,vertexColors:x.vertexColors,vertexAlphas:x.vertexColors===!0&&!!H.attributes.color&&H.attributes.color.itemSize===4,pointsUvs:G.isPoints===!0&&!!H.attributes.uv&&(Ft||$),fog:!!U,useFog:x.fog===!0,fogExp2:!!U&&U.isFogExp2,flatShading:x.wireframe===!1&&(x.flatShading===!0||H.attributes.normal===void 0&&oe===!1&&(x.isMeshLambertMaterial||x.isMeshPhongMaterial||x.isMeshStandardMaterial||x.isMeshPhysicalMaterial)),sizeAttenuation:x.sizeAttenuation===!0,logarithmicDepthBuffer:p,reversedDepthBuffer:gt,skinning:G.isSkinnedMesh===!0,hasPositionAttribute:H.attributes.position!==void 0,morphTargets:H.morphAttributes.position!==void 0,morphNormals:H.morphAttributes.normal!==void 0,morphColors:H.morphAttributes.color!==void 0,morphTargetsCount:At,morphTextureStride:bt,numSunLights:T.sun.length,numDirLights:T.directional.length,numPointLights:T.point.length,numSpotLights:T.spot.length,numSpotLightMaps:T.spotLightMap.length,numRectAreaLights:T.rectArea.length,numHemiLights:T.hemi.length,numSunLightShadows:T.sunShadowMap.length,numDirLightShadows:T.directionalShadowMap.length,numPointLightShadows:T.pointShadowMap.length,numSpotLightShadows:T.spotShadowMap.length,numSpotLightShadowsWithMaps:T.numSpotLightShadowsWithMaps,numLightProbes:T.numLightProbes,numLightProbeGrids:k.length,numClippingPlanes:s.numPlanes,numClipIntersection:s.numIntersection,dithering:x.dithering,shadowMapEnabled:i.shadowMap.enabled&&L.length>0,shadowMapType:i.shadowMap.type,toneMapping:Tt,decodeVideoTexture:Ft&&x.map.isVideoTexture===!0&&Ht.getTransfer(x.map.colorSpace)===$t,decodeVideoTextureEmissive:Le&&x.emissiveMap.isVideoTexture===!0&&Ht.getTransfer(x.emissiveMap.colorSpace)===$t,premultipliedAlpha:x.premultipliedAlpha,doubleSided:x.side===gn,flipSided:x.side===Ue,useDepthPacking:x.depthPacking>=0,depthPacking:x.depthPacking||0,index0AttributeName:x.index0AttributeName,extensionClipCullDistance:tt&&x.extensions.clipCullDistance===!0&&e.has("WEBGL_clip_cull_distance"),extensionMultiDraw:(tt&&x.extensions.multiDraw===!0||mt)&&e.has("WEBGL_multi_draw"),rendererExtensionParallelShaderCompile:e.has("KHR_parallel_shader_compile"),customProgramCacheKey:x.customProgramCacheKey()};return St.vertexUv1s=c.has(1),St.vertexUv2s=c.has(2),St.vertexUv3s=c.has(3),c.clear(),St}function m(x){const T=[];if(x.shaderID?T.push(x.shaderID):(T.push(x.customVertexShaderID),T.push(x.customFragmentShaderID)),x.defines!==void 0)for(const L in x.defines)T.push(L),T.push(x.defines[L]);return x.isRawShaderMaterial===!1&&(u(T,x),A(T,x),T.push(i.outputColorSpace)),T.push(x.customProgramCacheKey),T.join()}function u(x,T){x.push(T.precision),x.push(T.outputColorSpace),x.push(T.envMapMode),x.push(T.envMapCubeUVHeight),x.push(T.mapUv),x.push(T.alphaMapUv),x.push(T.lightMapUv),x.push(T.aoMapUv),x.push(T.bumpMapUv),x.push(T.normalMapUv),x.push(T.displacementMapUv),x.push(T.emissiveMapUv),x.push(T.metalnessMapUv),x.push(T.roughnessMapUv),x.push(T.anisotropyMapUv),x.push(T.clearcoatMapUv),x.push(T.clearcoatNormalMapUv),x.push(T.clearcoatRoughnessMapUv),x.push(T.iridescenceMapUv),x.push(T.iridescenceThicknessMapUv),x.push(T.sheenColorMapUv),x.push(T.sheenRoughnessMapUv),x.push(T.specularMapUv),x.push(T.specularColorMapUv),x.push(T.specularIntensityMapUv),x.push(T.transmissionMapUv),x.push(T.thicknessMapUv),x.push(T.combine),x.push(T.fogExp2),x.push(T.sizeAttenuation),x.push(T.morphTargetsCount),x.push(T.morphAttributeCount),x.push(T.numSunLights),x.push(T.numDirLights),x.push(T.numPointLights),x.push(T.numSpotLights),x.push(T.numSpotLightMaps),x.push(T.numHemiLights),x.push(T.numRectAreaLights),x.push(T.numSunLightShadows),x.push(T.numDirLightShadows),x.push(T.numPointLightShadows),x.push(T.numSpotLightShadows),x.push(T.numSpotLightShadowsWithMaps),x.push(T.numLightProbes),x.push(T.shadowMapType),x.push(T.toneMapping),x.push(T.numClippingPlanes),x.push(T.numClipIntersection),x.push(T.depthPacking)}function A(x,T){a.disableAll(),T.instancing&&a.enable(0),T.instancingColor&&a.enable(1),T.instancingMorph&&a.enable(2),T.matcap&&a.enable(3),T.envMap&&a.enable(4),T.normalMapObjectSpace&&a.enable(5),T.normalMapTangentSpace&&a.enable(6),T.clearcoat&&a.enable(7),T.iridescence&&a.enable(8),T.alphaTest&&a.enable(9),T.vertexColors&&a.enable(10),T.vertexAlphas&&a.enable(11),T.vertexUv1s&&a.enable(12),T.vertexUv2s&&a.enable(13),T.vertexUv3s&&a.enable(14),T.vertexTangents&&a.enable(15),T.anisotropy&&a.enable(16),T.alphaHash&&a.enable(17),T.batching&&a.enable(18),T.dispersion&&a.enable(19),T.retroreflection&&a.enable(24),T.batchingColor&&a.enable(20),T.gradientMap&&a.enable(21),T.packedNormalMap&&a.enable(22),T.vertexNormals&&a.enable(23),x.push(a.mask),a.disableAll(),T.fog&&a.enable(0),T.useFog&&a.enable(1),T.flatShading&&a.enable(2),T.logarithmicDepthBuffer&&a.enable(3),T.reversedDepthBuffer&&a.enable(4),T.skinning&&a.enable(5),T.morphTargets&&a.enable(6),T.morphNormals&&a.enable(7),T.morphColors&&a.enable(8),T.premultipliedAlpha&&a.enable(9),T.shadowMapEnabled&&a.enable(10),T.doubleSided&&a.enable(11),T.flipSided&&a.enable(12),T.useDepthPacking&&a.enable(13),T.dithering&&a.enable(14),T.transmission&&a.enable(15),T.sheen&&a.enable(16),T.opaque&&a.enable(17),T.pointsUvs&&a.enable(18),T.decodeVideoTexture&&a.enable(19),T.decodeVideoTextureEmissive&&a.enable(20),T.alphaToCoverage&&a.enable(21),T.numLightProbeGrids>0&&a.enable(22),T.hasPositionAttribute&&a.enable(23),x.push(a.mask)}function P(x){const T=_[x.type];let L;if(T){const F=sn[T];L=yh.clone(F.uniforms)}else L=x.uniforms;return L}function S(x,T){let L=d.get(T);return L!==void 0?++L.usedTimes:(L=new nm(i,T,x,r),l.push(L),d.set(T,L)),L}function b(x){if(--x.usedTimes===0){const T=l.indexOf(x);l[T]=l[l.length-1],l.pop(),d.delete(x.cacheKey),x.destroy()}}function y(x){o.remove(x)}function w(){o.dispose()}return{getParameters:M,getProgramCacheKey:m,getUniforms:P,acquireProgram:S,releaseProgram:b,releaseShaderCache:y,programs:l,dispose:w}}function lm(){let i=new WeakMap;function t(a){return i.has(a)}function e(a){let o=i.get(a);return o===void 0&&(o={},i.set(a,o)),o}function n(a){i.delete(a)}function r(a,o,c){i.get(a)[o]=c}function s(){i=new WeakMap}return{has:t,get:e,remove:n,update:r,dispose:s}}function cm(i,t){return i.groupOrder!==t.groupOrder?i.groupOrder-t.groupOrder:i.renderOrder!==t.renderOrder?i.renderOrder-t.renderOrder:i.material.id!==t.material.id?i.material.id-t.material.id:i.materialVariant!==t.materialVariant?i.materialVariant-t.materialVariant:i.z!==t.z?i.z-t.z:i.id-t.id}function $o(i,t){return i.groupOrder!==t.groupOrder?i.groupOrder-t.groupOrder:i.renderOrder!==t.renderOrder?i.renderOrder-t.renderOrder:i.z!==t.z?t.z-i.z:i.id-t.id}function Jo(){const i=[];let t=0;const e=[],n=[],r=[];function s(){t=0,e.length=0,n.length=0,r.length=0}function a(h){let _=0;return h.isInstancedMesh&&(_+=2),h.isSkinnedMesh&&(_+=1),_}function o(h,_,v,M,m,u){let A=i[t];return A===void 0?(A={id:h.id,object:h,geometry:_,material:v,materialVariant:a(h),groupOrder:M,renderOrder:h.renderOrder,z:m,group:u},i[t]=A):(A.id=h.id,A.object=h,A.geometry=_,A.material=v,A.materialVariant=a(h),A.groupOrder=M,A.renderOrder=h.renderOrder,A.z=m,A.group=u),t++,A}function c(h,_,v,M,m,u,A){A.reversedDepth===!0&&(m=-m);const P=o(h,_,v,M,m,u);v.transmission>0?n.push(P):v.transparent===!0?r.push(P):e.push(P)}function l(h,_,v,M,m,u){const A=o(h,_,v,M,m,u);v.transmission>0?n.unshift(A):v.transparent===!0?r.unshift(A):e.unshift(A)}function d(h,_){e.length>1&&e.sort(h||cm),n.length>1&&n.sort(_||$o),r.length>1&&r.sort(_||$o)}function p(){for(let h=t,_=i.length;h<_;h++){const v=i[h];if(v.id===null)break;v.id=null,v.object=null,v.geometry=null,v.material=null,v.group=null}}return{opaque:e,transmissive:n,transparent:r,init:s,push:c,unshift:l,finish:p,sort:d}}function hm(){let i=new WeakMap;function t(n,r){const s=i.get(n);let a;return s===void 0?(a=new Jo,i.set(n,[a])):r>=s.length?(a=new Jo,s.push(a)):a=s[r],a}function e(){i=new WeakMap}return{get:t,dispose:e}}function um(){const i={};return{get:function(t){if(i[t.id]!==void 0)return i[t.id];let e;switch(t.type){case"SunLight":case"DirectionalLight":e={direction:new I,color:new Gt};break;case"SpotLight":e={position:new I,direction:new I,color:new Gt,distance:0,coneCos:0,penumbraCos:0,decay:0};break;case"PointLight":e={position:new I,color:new Gt,distance:0,decay:0};break;case"HemisphereLight":e={direction:new I,skyColor:new Gt,groundColor:new Gt};break;case"RectAreaLight":e={color:new Gt,position:new I,halfWidth:new I,halfHeight:new I};break}return i[t.id]=e,e}}}function dm(){const i={};return{get:function(t){if(i[t.id]!==void 0)return i[t.id];let e;switch(t.type){case"SunLight":case"DirectionalLight":e={shadowIntensity:1,shadowBias:0,shadowNormalBias:0,shadowRadius:1,shadowMapSize:new Ct};break;case"SpotLight":e={shadowIntensity:1,shadowBias:0,shadowNormalBias:0,shadowRadius:1,shadowMapSize:new Ct};break;case"PointLight":e={shadowIntensity:1,shadowBias:0,shadowNormalBias:0,shadowRadius:1,shadowMapSize:new Ct,shadowCameraNear:1,shadowCameraFar:1e3};break}return i[t.id]=e,e}}}let fm=0;function pm(i,t){return(t.castShadow?2:0)-(i.castShadow?2:0)+(t.map?1:0)-(i.map?1:0)}function mm(i){const t=new um,e=dm(),n={version:0,hash:{sunLength:-1,directionalLength:-1,pointLength:-1,spotLength:-1,rectAreaLength:-1,hemiLength:-1,numSunShadows:-1,numDirectionalShadows:-1,numPointShadows:-1,numSpotShadows:-1,numSpotMaps:-1,numLightProbes:-1},ambient:[0,0,0],probe:[],sun:[],sunShadow:[],sunShadowMap:[],sunShadowMatrix:[],sunShadowCascade:[],directional:[],directionalShadow:[],directionalShadowMap:[],directionalShadowMatrix:[],spot:[],spotLightMap:[],spotShadow:[],spotShadowMap:[],spotLightMatrix:[],rectArea:[],rectAreaLTC1:null,rectAreaLTC2:null,point:[],pointShadow:[],pointShadowMap:[],pointShadowMatrix:[],hemi:[],numSpotLightShadowsWithMaps:0,numLightProbes:0};for(let l=0;l<9;l++)n.probe.push(new I);const r=new I,s=new ee,a=new ee;function o(l){let d=0,p=0,h=0;for(let G=0;G<9;G++)n.probe[G].set(0,0,0);let _=0,v=0,M=0,m=0,u=0,A=0,P=0,S=0,b=0,y=0,w=0,x=0,T=0,L=0;l.sort(pm);for(let G=0,k=l.length;G<k;G++){const U=l[G],H=U.color,K=U.intensity,Z=U.distance;let nt=null;if(U.shadow&&U.shadow.map&&(U.shadow.map.texture.format===Xn?nt=U.shadow.map.texture:nt=U.shadow.map.depthTexture||U.shadow.map.texture),U.isAmbientLight)d+=H.r*K,p+=H.g*K,h+=H.b*K;else if(U.isLightProbe){for(let X=0;X<9;X++)n.probe[X].addScaledVector(U.sh.coefficients[X],K);L++}else if(U.isSunLight){const X=t.get(U);if(X.color.copy(U.color).multiplyScalar(U.intensity),U.castShadow){const j=U.shadow,et=e.get(U);et.shadowIntensity=j.intensity,et.shadowBias=j.bias,et.shadowNormalBias=j.normalBias,et.shadowRadius=j.radius,et.shadowMapSize.copy(j.mapSize).multiply(j.getFrameExtents()),n.sunShadow[v]=et,n.sunShadowMap[v]=nt;const At=j.getViewportCount();for(let bt=0;bt<At;bt++)n.sunShadowMatrix[M+bt]=j.getMatrix(bt),n.sunShadowCascade[M+bt]=j._cascadeData[bt];M+=At,v++}n.sun[_]=X,_++}else if(U.isDirectionalLight){const X=t.get(U);if(X.color.copy(U.color).multiplyScalar(U.intensity),U.castShadow){const j=U.shadow,et=e.get(U);et.shadowIntensity=j.intensity,et.shadowBias=j.bias,et.shadowNormalBias=j.normalBias,et.shadowRadius=j.radius,et.shadowMapSize=j.mapSize,n.directionalShadow[m]=et,n.directionalShadowMap[m]=nt,n.directionalShadowMatrix[m]=U.shadow.matrix,b++}n.directional[m]=X,m++}else if(U.isSpotLight){const X=t.get(U);X.position.setFromMatrixPosition(U.matrixWorld),X.color.copy(H).multiplyScalar(K),X.distance=Z,X.coneCos=Math.cos(U.angle),X.penumbraCos=Math.cos(U.angle*(1-U.penumbra)),X.decay=U.decay,n.spot[A]=X;const j=U.shadow;if(U.map&&(n.spotLightMap[x]=U.map,x++,j.updateMatrices(U),U.castShadow&&T++),n.spotLightMatrix[A]=j.matrix,U.castShadow){const et=e.get(U);et.shadowIntensity=j.intensity,et.shadowBias=j.bias,et.shadowNormalBias=j.normalBias,et.shadowRadius=j.radius,et.shadowMapSize=j.mapSize,n.spotShadow[A]=et,n.spotShadowMap[A]=nt,w++}A++}else if(U.isRectAreaLight){const X=t.get(U);X.color.copy(H).multiplyScalar(K),X.halfWidth.set(U.width*.5,0,0),X.halfHeight.set(0,U.height*.5,0),n.rectArea[P]=X,P++}else if(U.isPointLight){const X=t.get(U);if(X.color.copy(U.color).multiplyScalar(U.intensity),X.distance=U.distance,X.decay=U.decay,U.castShadow){const j=U.shadow,et=e.get(U);et.shadowIntensity=j.intensity,et.shadowBias=j.bias,et.shadowNormalBias=j.normalBias,et.shadowRadius=j.radius,et.shadowMapSize=j.mapSize,et.shadowCameraNear=j.camera.near,et.shadowCameraFar=j.camera.far,n.pointShadow[u]=et,n.pointShadowMap[u]=nt,n.pointShadowMatrix[u]=U.shadow.matrix,y++}n.point[u]=X,u++}else if(U.isHemisphereLight){const X=t.get(U);X.skyColor.copy(U.color).multiplyScalar(K),X.groundColor.copy(U.groundColor).multiplyScalar(K),n.hemi[S]=X,S++}}P>0&&(i.has("OES_texture_float_linear")===!0?(n.rectAreaLTC1=ht.LTC_FLOAT_1,n.rectAreaLTC2=ht.LTC_FLOAT_2):(n.rectAreaLTC1=ht.LTC_HALF_1,n.rectAreaLTC2=ht.LTC_HALF_2)),n.ambient[0]=d,n.ambient[1]=p,n.ambient[2]=h;const F=n.hash;(F.sunLength!==_||F.directionalLength!==m||F.pointLength!==u||F.spotLength!==A||F.rectAreaLength!==P||F.hemiLength!==S||F.numSunShadows!==v||F.numDirectionalShadows!==b||F.numPointShadows!==y||F.numSpotShadows!==w||F.numSpotMaps!==x||F.numLightProbes!==L)&&(n.sun.length=_,n.directional.length=m,n.spot.length=A,n.rectArea.length=P,n.point.length=u,n.hemi.length=S,n.sunShadow.length=v,n.sunShadowMap.length=v,n.sunShadowMatrix.length=M,n.sunShadowCascade.length=M,n.directionalShadow.length=b,n.directionalShadowMap.length=b,n.directionalShadowMatrix.length=b,n.pointShadow.length=y,n.pointShadowMap.length=y,n.pointShadowMatrix.length=y,n.spotShadow.length=w,n.spotShadowMap.length=w,n.spotLightMatrix.length=w+x-T,n.spotLightMap.length=x,n.numSpotLightShadowsWithMaps=T,n.numLightProbes=L,F.sunLength=_,F.directionalLength=m,F.pointLength=u,F.spotLength=A,F.rectAreaLength=P,F.hemiLength=S,F.numSunShadows=v,F.numDirectionalShadows=b,F.numPointShadows=y,F.numSpotShadows=w,F.numSpotMaps=x,F.numLightProbes=L,n.version=fm++)}function c(l,d){let p=0,h=0,_=0,v=0,M=0,m=0;const u=d.matrixWorldInverse;for(let A=0,P=l.length;A<P;A++){const S=l[A];if(S.isSunLight){const b=n.sun[p];b.direction.setFromMatrixPosition(S.matrixWorld),b.direction.transformDirection(u),p++}else if(S.isDirectionalLight){const b=n.directional[h];b.direction.setFromMatrixPosition(S.matrixWorld),r.setFromMatrixPosition(S.target.matrixWorld),b.direction.sub(r),b.direction.transformDirection(u),h++}else if(S.isSpotLight){const b=n.spot[v];b.position.setFromMatrixPosition(S.matrixWorld),b.position.applyMatrix4(u),b.direction.setFromMatrixPosition(S.matrixWorld),r.setFromMatrixPosition(S.target.matrixWorld),b.direction.sub(r),b.direction.transformDirection(u),v++}else if(S.isRectAreaLight){const b=n.rectArea[M];b.position.setFromMatrixPosition(S.matrixWorld),b.position.applyMatrix4(u),a.identity(),s.copy(S.matrixWorld),s.premultiply(u),a.extractRotation(s),b.halfWidth.set(S.width*.5,0,0),b.halfHeight.set(0,S.height*.5,0),b.halfWidth.applyMatrix4(a),b.halfHeight.applyMatrix4(a),M++}else if(S.isPointLight){const b=n.point[_];b.position.setFromMatrixPosition(S.matrixWorld),b.position.applyMatrix4(u),_++}else if(S.isHemisphereLight){const b=n.hemi[m];b.direction.setFromMatrixPosition(S.matrixWorld),b.direction.transformDirection(u),m++}}}return{setup:o,setupView:c,state:n}}function Qo(i){const t=new mm(i),e=[],n=[],r=[];function s(h){p.camera=h,e.length=0,n.length=0,r.length=0}function a(h){e.push(h)}function o(h){n.push(h)}function c(h){r.push(h)}function l(){t.setup(e)}function d(h){t.setupView(e,h)}const p={lightsArray:e,shadowsArray:n,lightProbeGridArray:r,camera:null,lights:t,transmissionRenderTarget:{},textureUnits:0};return{init:s,state:p,setupLights:l,setupLightsView:d,pushLight:a,pushShadow:o,pushLightProbeGrid:c}}function _m(i){let t=new WeakMap;function e(r,s=0){const a=t.get(r);let o;return a===void 0?(o=new Qo(i),t.set(r,[o])):s>=a.length?(o=new Qo(i),a.push(o)):o=a[s],o}function n(){t=new WeakMap}return{get:e,dispose:n}}const gm=`void main() {
	gl_Position = vec4( position, 1.0 );
}`,xm=`uniform sampler2D shadow_pass;
uniform vec2 resolution;
uniform float radius;
void main() {
	const float samples = float( VSM_SAMPLES );
	float mean = 0.0;
	float squared_mean = 0.0;
	float uvStride = samples <= 1.0 ? 0.0 : 2.0 / ( samples - 1.0 );
	float uvStart = samples <= 1.0 ? 0.0 : - 1.0;
	for ( float i = 0.0; i < samples; i ++ ) {
		float uvOffset = uvStart + i * uvStride;
		#ifdef HORIZONTAL_PASS
			vec2 distribution = texture2D( shadow_pass, ( gl_FragCoord.xy + vec2( uvOffset, 0.0 ) * radius ) / resolution ).rg;
			mean += distribution.x;
			squared_mean += distribution.y * distribution.y + distribution.x * distribution.x;
		#else
			float depth = texture2D( shadow_pass, ( gl_FragCoord.xy + vec2( 0.0, uvOffset ) * radius ) / resolution ).r;
			mean += depth;
			squared_mean += depth * depth;
		#endif
	}
	mean = mean / samples;
	squared_mean = squared_mean / samples;
	float std_dev = sqrt( max( 0.0, squared_mean - mean * mean ) );
	gl_FragColor = vec4( mean, std_dev, 0.0, 1.0 );
}`,vm=[new I(1,0,0),new I(-1,0,0),new I(0,1,0),new I(0,-1,0),new I(0,0,1),new I(0,0,-1)],Sm=[new I(0,-1,0),new I(0,-1,0),new I(0,0,1),new I(0,0,-1),new I(0,-1,0),new I(0,-1,0)],jo=new ee,Di=new I,ys=new I;function Mm(i,t,e){let n=new Aa;const r=new Ct,s=new Ct,a=new ce,o=new wh,c=new Rh,l={},d=e.maxTextureSize,p={[kn]:Ue,[Ue]:kn,[gn]:gn},h=new hn({defines:{VSM_SAMPLES:8},uniforms:{shadow_pass:{value:null},resolution:{value:new Ct},radius:{value:4}},vertexShader:gm,fragmentShader:xm}),_=h.clone();_.defines.HORIZONTAL_PASS=1;const v=new Ne;v.setAttribute("position",new Qe(new Float32Array([-1,-1,.5,3,-1,.5,-1,3,.5]),3));const M=new je(v,h),m=this;this.enabled=!1,this.autoUpdate=!0,this.needsUpdate=!1,this.type=Er;let u=this.type;this.render=function(y,w,x){if(m.enabled===!1||m.autoUpdate===!1&&m.needsUpdate===!1||y.length===0)return;this.type===Ql&&(Rt("WebGLShadowMap: PCFSoftShadowMap has been removed. Using PCFShadowMap instead."),this.type=Er);const T=i.getRenderTarget(),L=i.getActiveCubeFace(),F=i.getActiveMipmapLevel(),G=i.state;G.setBlending(vn),G.buffers.depth.getReversed()===!0?G.buffers.color.setClear(0,0,0,0):G.buffers.color.setClear(1,1,1,1),G.buffers.depth.setTest(!0),G.setScissorTest(!1);const k=u!==this.type;k&&w.traverse(function(U){U.material&&(Array.isArray(U.material)?U.material.forEach(H=>H.needsUpdate=!0):U.material.needsUpdate=!0)});for(let U=0,H=y.length;U<H;U++){const K=y[U],Z=K.shadow;if(Z===void 0){Rt("WebGLShadowMap:",K,"has no shadow.");continue}if(Z.autoUpdate===!1&&Z.needsUpdate===!1)continue;r.copy(Z.mapSize);const nt=Z.getFrameExtents();r.multiply(nt),s.copy(Z.mapSize),(r.x>d||r.y>d)&&(r.x>d&&(s.x=Math.floor(d/nt.x),r.x=s.x*nt.x,Z.mapSize.x=s.x),r.y>d&&(s.y=Math.floor(d/nt.y),r.y=s.y*nt.y,Z.mapSize.y=s.y));const X=i.state.buffers.depth.getReversed();if(Z.camera._reversedDepth=X,Z.map===null||k===!0){if(Z.map!==null&&(Z.map.depthTexture!==null&&(Z.map.depthTexture.dispose(),Z.map.depthTexture=null),Z.map.dispose()),this.type===Li){if(K.isPointLight){Rt("WebGLShadowMap: VSM shadow maps are not supported for PointLights. Use PCF or BasicShadowMap instead.");continue}Z.map=new Je(r.x,r.y,{format:Xn,type:cn,minFilter:Ae,magFilter:Ae,generateMipmaps:!1}),Z.map.texture.name=K.name+".shadowMap",Z.map.depthTexture=new Hi(r.x,r.y,Ke),Z.map.depthTexture.name=K.name+".shadowMapDepth",Z.map.depthTexture.format=En,Z.map.depthTexture.compareFunction=null,Z.map.depthTexture.minFilter=Me,Z.map.depthTexture.magFilter=Me}else K.isPointLight?(Z.map=new Fl(r.x),Z.map.depthTexture=new Mh(r.x,ln)):(Z.map=new Je(r.x,r.y),Z.map.depthTexture=new Hi(r.x,r.y,ln)),Z.map.depthTexture.name=K.name+".shadowMap",Z.map.depthTexture.format=En,this.type===Er?(Z.map.depthTexture.compareFunction=X?Ea:Ma,Z.map.depthTexture.minFilter=Ae,Z.map.depthTexture.magFilter=Ae):(Z.map.depthTexture.compareFunction=null,Z.map.depthTexture.minFilter=Me,Z.map.depthTexture.magFilter=Me);Z.camera.updateProjectionMatrix()}Z.map.isWebGLCubeRenderTarget!==!0&&(Z.map.width!==r.x||Z.map.height!==r.y)&&Z.map.setSize(r.x,r.y);const j=Z.map.isWebGLCubeRenderTarget?6:Z.getViewportCount();K.isPointLight!==!0&&Z.updateMatrices(K,x);for(let et=0;et<j;et++){const At=Z.getCamera(et);if(K.isPointLight){const bt=Z.camera,ne=Z.matrix,Vt=K.distance||bt.far;Vt!==bt.far&&(bt.far=Vt,bt.updateProjectionMatrix()),Di.setFromMatrixPosition(K.matrixWorld),bt.position.copy(Di),ys.copy(bt.position),ys.add(vm[et]),bt.up.copy(Sm[et]),bt.lookAt(ys),bt.updateMatrixWorld(),ne.makeTranslation(-Di.x,-Di.y,-Di.z),jo.multiplyMatrices(bt.projectionMatrix,bt.matrixWorldInverse),Z._frustum.setFromProjectionMatrix(jo,bt.coordinateSystem,bt.reversedDepth)}if(Z.map.isWebGLCubeRenderTarget)i.setRenderTarget(Z.map,et),i.clear();else{et===0&&(i.setRenderTarget(Z.map),i.clear());const bt=Z.getViewport(et);a.set(s.x*bt.x,s.y*bt.y,s.x*bt.z,s.y*bt.w),G.viewport(a)}n=Z.getFrustum(et),S(w,x,At,K,this.type)}Z.isPointLightShadow!==!0&&this.type===Li&&A(Z,x),Z.needsUpdate=!1}u=this.type,m.needsUpdate=!1,i.setRenderTarget(T,L,F)};function A(y,w){const x=t.update(M);h.defines.VSM_SAMPLES!==y.blurSamples&&(h.defines.VSM_SAMPLES=y.blurSamples,_.defines.VSM_SAMPLES=y.blurSamples,h.needsUpdate=!0,_.needsUpdate=!0),y.mapPass===null?y.mapPass=new Je(r.x,r.y,{format:Xn,type:cn}):(y.mapPass.width!==y.map.width||y.mapPass.height!==y.map.height)&&y.mapPass.setSize(y.map.width,y.map.height),h.uniforms.shadow_pass.value=y.map.depthTexture,h.uniforms.resolution.value.set(y.map.width,y.map.height),h.uniforms.radius.value=y.radius,i.setRenderTarget(y.mapPass),i.clear(),i.renderBufferDirect(w,null,x,h,M,null),_.uniforms.shadow_pass.value=y.mapPass.texture,_.uniforms.resolution.value.set(y.map.width,y.map.height),_.uniforms.radius.value=y.radius,i.setRenderTarget(y.map),i.clear(),i.renderBufferDirect(w,null,x,_,M,null)}function P(y,w,x,T){let L=null;const F=x.isPointLight===!0?y.customDistanceMaterial:y.customDepthMaterial;if(F!==void 0)L=F;else if(L=x.isPointLight===!0?c:o,i.localClippingEnabled&&w.clipShadows===!0&&Array.isArray(w.clippingPlanes)&&w.clippingPlanes.length!==0||w.displacementMap&&w.displacementScale!==0||w.alphaMap&&w.alphaTest>0||w.map&&w.alphaTest>0||w.alphaToCoverage===!0){const G=L.uuid,k=w.uuid;let U=l[G];U===void 0&&(U={},l[G]=U);let H=U[k];H===void 0&&(H=L.clone(),U[k]=H,w.addEventListener("dispose",b)),L=H}if(L.visible=w.visible,L.wireframe=w.wireframe,T===Li?L.side=w.shadowSide!==null?w.shadowSide:w.side:L.side=w.shadowSide!==null?w.shadowSide:p[w.side],L.alphaMap=w.alphaMap,L.alphaTest=w.alphaToCoverage===!0?.5:w.alphaTest,L.map=w.map,L.clipShadows=w.clipShadows,L.clippingPlanes=w.clippingPlanes,L.clipIntersection=w.clipIntersection,L.displacementMap=w.displacementMap,L.displacementScale=w.displacementScale,L.displacementBias=w.displacementBias,L.wireframeLinewidth=w.wireframeLinewidth,L.linewidth=w.linewidth,x.isPointLight===!0&&L.isMeshDistanceMaterial===!0){const G=i.properties.get(L);G.light=x}return L}function S(y,w,x,T,L){if(y.visible===!1)return;if(y.layers.test(w.layers)&&(y.isMesh||y.isLine||y.isPoints)&&(y.castShadow||y.receiveShadow&&L===Li)&&(!y.frustumCulled||y.intersectsFrustum(n))){y.modelViewMatrix.multiplyMatrices(x.matrixWorldInverse,y.matrixWorld);const k=t.update(y),U=y.material;if(Array.isArray(U)){const H=k.groups;for(let K=0,Z=H.length;K<Z;K++){const nt=H[K],X=U[nt.materialIndex];if(X&&X.visible){const j=P(y,X,T,L);y.onBeforeShadow(i,y,w,x,k,j,nt),i.renderBufferDirect(x,null,k,j,y,nt),y.onAfterShadow(i,y,w,x,k,j,nt)}}}else if(U.visible){const H=P(y,U,T,L);y.onBeforeShadow(i,y,w,x,k,H,null),i.renderBufferDirect(x,null,k,H,y,null),y.onAfterShadow(i,y,w,x,k,H,null)}}const G=y.children;for(let k=0,U=G.length;k<U;k++)S(G[k],w,x,T,L)}function b(y){y.target.removeEventListener("dispose",b);for(const x in l){const T=l[x],L=y.target.uuid;L in T&&(T[L].dispose(),delete T[L])}}}function Em(i,t){function e(){let C=!1;const ot=new ce;let $=null;const lt=new ce(0,0,0,0);return{setMask:function(ft){$!==ft&&!C&&(i.colorMask(ft,ft,ft,ft),$=ft)},setLocked:function(ft){C=ft},setClear:function(ft,tt,Tt,St,re){re===!0&&(ft*=St,tt*=St,Tt*=St),ot.set(ft,tt,Tt,St),lt.equals(ot)===!1&&(i.clearColor(ft,tt,Tt,St),lt.copy(ot))},reset:function(){C=!1,$=null,lt.set(-1,0,0,0)}}}function n(){let C=!1,ot=!1,$=null,lt=null,ft=null;return{setReversed:function(tt){if(ot!==tt){const Tt=t.get("EXT_clip_control");tt?Tt.clipControlEXT(Tt.LOWER_LEFT_EXT,Tt.ZERO_TO_ONE_EXT):Tt.clipControlEXT(Tt.LOWER_LEFT_EXT,Tt.NEGATIVE_ONE_TO_ONE_EXT),ot=tt;const St=ft;ft=null,this.setClear(St)}},getReversed:function(){return ot},setTest:function(tt){tt?Q(i.DEPTH_TEST):gt(i.DEPTH_TEST)},setMask:function(tt){$!==tt&&!C&&(i.depthMask(tt),$=tt)},setFunc:function(tt){if(ot&&(tt=Lc[tt]),lt!==tt){switch(tt){case Ts:i.depthFunc(i.NEVER);break;case As:i.depthFunc(i.ALWAYS);break;case ws:i.depthFunc(i.LESS);break;case Fi:i.depthFunc(i.LEQUAL);break;case Rs:i.depthFunc(i.EQUAL);break;case Cs:i.depthFunc(i.GEQUAL);break;case Ps:i.depthFunc(i.GREATER);break;case Ds:i.depthFunc(i.NOTEQUAL);break;default:i.depthFunc(i.LEQUAL)}lt=tt}},setLocked:function(tt){C=tt},setClear:function(tt){ft!==tt&&(ft=tt,ot&&(tt=1-tt),i.clearDepth(tt))},reset:function(){C=!1,$=null,lt=null,ft=null,ot=!1}}}function r(){let C=!1,ot=null,$=null,lt=null,ft=null,tt=null,Tt=null,St=null,re=null;return{setTest:function(qt){C||(qt?Q(i.STENCIL_TEST):gt(i.STENCIL_TEST))},setMask:function(qt){ot!==qt&&!C&&(i.stencilMask(qt),ot=qt)},setFunc:function(qt,ke,tn){($!==qt||lt!==ke||ft!==tn)&&(i.stencilFunc(qt,ke,tn),$=qt,lt=ke,ft=tn)},setOp:function(qt,ke,tn){(tt!==qt||Tt!==ke||St!==tn)&&(i.stencilOp(qt,ke,tn),tt=qt,Tt=ke,St=tn)},setLocked:function(qt){C=qt},setClear:function(qt){re!==qt&&(i.clearStencil(qt),re=qt)},reset:function(){C=!1,ot=null,$=null,lt=null,ft=null,tt=null,Tt=null,St=null,re=null}}}const s=new e,a=new n,o=new r,c=new WeakMap,l=new WeakMap;let d={},p={},h={},_=new WeakMap,v=[],M=null,m=!1,u=null,A=null,P=null,S=null,b=null,y=null,w=null,x=new Gt(0,0,0),T=0,L=!1,F=null,G=null,k=null,U=null,H=null;const K=i.getParameter(i.MAX_COMBINED_TEXTURE_IMAGE_UNITS);let Z=!1,nt=0;const X=i.getParameter(i.VERSION);X.indexOf("WebGL")!==-1?(nt=parseFloat(/^WebGL (\d)/.exec(X)[1]),Z=nt>=1):X.indexOf("OpenGL ES")!==-1&&(nt=parseFloat(/^OpenGL ES (\d)/.exec(X)[1]),Z=nt>=2);let j=null,et={};const At=i.getParameter(i.SCISSOR_BOX),bt=i.getParameter(i.VIEWPORT),ne=new ce().fromArray(At),Vt=new ce().fromArray(bt);function Yt(C,ot,$,lt){const ft=new Uint8Array(4),tt=i.createTexture();i.bindTexture(C,tt),i.texParameteri(C,i.TEXTURE_MIN_FILTER,i.NEAREST),i.texParameteri(C,i.TEXTURE_MAG_FILTER,i.NEAREST);for(let Tt=0;Tt<$;Tt++)C===i.TEXTURE_3D||C===i.TEXTURE_2D_ARRAY?i.texImage3D(ot,0,i.RGBA,1,1,lt,0,i.RGBA,i.UNSIGNED_BYTE,ft):i.texImage2D(ot+Tt,0,i.RGBA,1,1,0,i.RGBA,i.UNSIGNED_BYTE,ft);return tt}const Y={};Y[i.TEXTURE_2D]=Yt(i.TEXTURE_2D,i.TEXTURE_2D,1),Y[i.TEXTURE_CUBE_MAP]=Yt(i.TEXTURE_CUBE_MAP,i.TEXTURE_CUBE_MAP_POSITIVE_X,6),Y[i.TEXTURE_2D_ARRAY]=Yt(i.TEXTURE_2D_ARRAY,i.TEXTURE_2D_ARRAY,1,1),Y[i.TEXTURE_3D]=Yt(i.TEXTURE_3D,i.TEXTURE_3D,1,1),s.setClear(0,0,0,1),a.setClear(1),o.setClear(0),Q(i.DEPTH_TEST),a.setFunc(Fi),zt(!1),oe(Xa),Q(i.CULL_FACE),Xt(vn);function Q(C){d[C]!==!0&&(i.enable(C),d[C]=!0)}function gt(C){d[C]!==!1&&(i.disable(C),d[C]=!1)}function Pt(C,ot){return h[C]!==ot?(i.bindFramebuffer(C,ot),h[C]=ot,C===i.DRAW_FRAMEBUFFER&&(h[i.FRAMEBUFFER]=ot),C===i.FRAMEBUFFER&&(h[i.DRAW_FRAMEBUFFER]=ot),!0):!1}function mt(C,ot){let $=v,lt=!1;if(C){$=_.get(ot),$===void 0&&($=[],_.set(ot,$));const ft=C.textures;if($.length!==ft.length||$[0]!==i.COLOR_ATTACHMENT0){for(let tt=0,Tt=ft.length;tt<Tt;tt++)$[tt]=i.COLOR_ATTACHMENT0+tt;$.length=ft.length,lt=!0}}else $[0]!==i.BACK&&($[0]=i.BACK,lt=!0);lt&&i.drawBuffers($)}function Ft(C){return M!==C?(i.useProgram(C),M=C,!0):!1}const pe={[ui]:i.FUNC_ADD,[tc]:i.FUNC_SUBTRACT,[ec]:i.FUNC_REVERSE_SUBTRACT};pe[nc]=i.MIN,pe[ic]=i.MAX;const Ot={[rc]:i.ZERO,[sc]:i.ONE,[ac]:i.SRC_COLOR,[nl]:i.SRC_ALPHA,[dc]:i.SRC_ALPHA_SATURATE,[hc]:i.DST_COLOR,[lc]:i.DST_ALPHA,[oc]:i.ONE_MINUS_SRC_COLOR,[il]:i.ONE_MINUS_SRC_ALPHA,[uc]:i.ONE_MINUS_DST_COLOR,[cc]:i.ONE_MINUS_DST_ALPHA,[fc]:i.CONSTANT_COLOR,[pc]:i.ONE_MINUS_CONSTANT_COLOR,[mc]:i.CONSTANT_ALPHA,[_c]:i.ONE_MINUS_CONSTANT_ALPHA};function Xt(C,ot,$,lt,ft,tt,Tt,St,re,qt){if(C===vn){m===!0&&(gt(i.BLEND),m=!1);return}if(m===!1&&(Q(i.BLEND),m=!0),C!==jl){if(C!==u||qt!==L){if((A!==ui||b!==ui)&&(i.blendEquation(i.FUNC_ADD),A=ui,b=ui),qt)switch(C){case Ui:i.blendFuncSeparate(i.ONE,i.ONE_MINUS_SRC_ALPHA,i.ONE,i.ONE_MINUS_SRC_ALPHA);break;case Ya:i.blendFunc(i.ONE,i.ONE);break;case qa:i.blendFuncSeparate(i.ZERO,i.ONE_MINUS_SRC_COLOR,i.ZERO,i.ONE);break;case Za:i.blendFuncSeparate(i.DST_COLOR,i.ONE_MINUS_SRC_ALPHA,i.ZERO,i.ONE);break;default:Wt("WebGLState: Invalid blending: ",C);break}else switch(C){case Ui:i.blendFuncSeparate(i.SRC_ALPHA,i.ONE_MINUS_SRC_ALPHA,i.ONE,i.ONE_MINUS_SRC_ALPHA);break;case Ya:i.blendFuncSeparate(i.SRC_ALPHA,i.ONE,i.ONE,i.ONE);break;case qa:Wt("WebGLState: SubtractiveBlending requires material.premultipliedAlpha = true");break;case Za:Wt("WebGLState: MultiplyBlending requires material.premultipliedAlpha = true");break;default:Wt("WebGLState: Invalid blending: ",C);break}P=null,S=null,y=null,w=null,x.set(0,0,0),T=0,u=C,L=qt}return}ft=ft||ot,tt=tt||$,Tt=Tt||lt,(ot!==A||ft!==b)&&(i.blendEquationSeparate(pe[ot],pe[ft]),A=ot,b=ft),($!==P||lt!==S||tt!==y||Tt!==w)&&(i.blendFuncSeparate(Ot[$],Ot[lt],Ot[tt],Ot[Tt]),P=$,S=lt,y=tt,w=Tt),(St.equals(x)===!1||re!==T)&&(i.blendColor(St.r,St.g,St.b,re),x.copy(St),T=re),u=C,L=!1}function ie(C,ot){C.side===gn?gt(i.CULL_FACE):Q(i.CULL_FACE);let $=C.side===Ue;ot&&($=!$),zt($),C.blending===Ui&&C.transparent===!1?Xt(vn):Xt(C.blending,C.blendEquation,C.blendSrc,C.blendDst,C.blendEquationAlpha,C.blendSrcAlpha,C.blendDstAlpha,C.blendColor,C.blendAlpha,C.premultipliedAlpha),a.setFunc(C.depthFunc),a.setTest(C.depthTest),a.setMask(C.depthWrite),s.setMask(C.colorWrite);const lt=C.stencilWrite;o.setTest(lt),lt&&(o.setMask(C.stencilWriteMask),o.setFunc(C.stencilFunc,C.stencilRef,C.stencilFuncMask),o.setOp(C.stencilFail,C.stencilZFail,C.stencilZPass)),Le(C.polygonOffset,C.polygonOffsetFactor,C.polygonOffsetUnits),C.alphaToCoverage===!0?Q(i.SAMPLE_ALPHA_TO_COVERAGE):gt(i.SAMPLE_ALPHA_TO_COVERAGE)}function zt(C){F!==C&&(C?i.frontFace(i.CW):i.frontFace(i.CCW),F=C)}function oe(C){C!==$l?(Q(i.CULL_FACE),C!==G&&(C===Xa?i.cullFace(i.BACK):C===Jl?i.cullFace(i.FRONT):i.cullFace(i.FRONT_AND_BACK))):gt(i.CULL_FACE),G=C}function xe(C){C!==k&&(Z&&i.lineWidth(C),k=C)}function Le(C,ot,$){C?(Q(i.POLYGON_OFFSET_FILL),(U!==ot||H!==$)&&(U=ot,H=$,a.getReversed()&&(ot=-ot),i.polygonOffset(ot,$))):gt(i.POLYGON_OFFSET_FILL)}function le(C){C?Q(i.SCISSOR_TEST):gt(i.SCISSOR_TEST)}function ue(C){C===void 0&&(C=i.TEXTURE0+K-1),j!==C&&(i.activeTexture(C),j=C)}function D(C,ot,$){$===void 0&&(j===null?$=i.TEXTURE0+K-1:$=j);let lt=et[$];lt===void 0&&(lt={type:void 0,texture:void 0},et[$]=lt),(lt.type!==C||lt.texture!==ot)&&(j!==$&&(i.activeTexture($),j=$),i.bindTexture(C,ot||Y[C]),lt.type=C,lt.texture=ot)}function Ee(){const C=et[j];C!==void 0&&C.type!==void 0&&(i.bindTexture(C.type,null),C.type=void 0,C.texture=void 0)}function Kt(){try{i.compressedTexImage2D(...arguments)}catch(C){Wt("WebGLState:",C)}}function E(){try{i.compressedTexImage3D(...arguments)}catch(C){Wt("WebGLState:",C)}}function f(){try{i.texSubImage2D(...arguments)}catch(C){Wt("WebGLState:",C)}}function N(){try{i.texSubImage3D(...arguments)}catch(C){Wt("WebGLState:",C)}}function z(){try{i.compressedTexSubImage2D(...arguments)}catch(C){Wt("WebGLState:",C)}}function W(){try{i.compressedTexSubImage3D(...arguments)}catch(C){Wt("WebGLState:",C)}}function it(){try{i.texStorage2D(...arguments)}catch(C){Wt("WebGLState:",C)}}function rt(){try{i.texStorage3D(...arguments)}catch(C){Wt("WebGLState:",C)}}function q(){try{i.texImage2D(...arguments)}catch(C){Wt("WebGLState:",C)}}function J(){try{i.texImage3D(...arguments)}catch(C){Wt("WebGLState:",C)}}function st(C){return p[C]!==void 0?p[C]:i.getParameter(C)}function Et(C,ot){p[C]!==ot&&(i.pixelStorei(C,ot),p[C]=ot)}function ct(C){ne.equals(C)===!1&&(i.scissor(C.x,C.y,C.z,C.w),ne.copy(C))}function at(C){Vt.equals(C)===!1&&(i.viewport(C.x,C.y,C.z,C.w),Vt.copy(C))}function yt(C,ot){let $=l.get(ot);$===void 0&&($=new WeakMap,l.set(ot,$));let lt=$.get(C);lt===void 0&&(lt=i.getUniformBlockIndex(ot,C.name),$.set(C,lt))}function wt(C,ot){const lt=l.get(ot).get(C);c.get(ot)!==lt&&(i.uniformBlockBinding(ot,lt,C.__bindingPointIndex),c.set(ot,lt))}function It(){i.disable(i.BLEND),i.disable(i.CULL_FACE),i.disable(i.DEPTH_TEST),i.disable(i.POLYGON_OFFSET_FILL),i.disable(i.SCISSOR_TEST),i.disable(i.STENCIL_TEST),i.disable(i.SAMPLE_ALPHA_TO_COVERAGE),i.blendEquation(i.FUNC_ADD),i.blendFunc(i.ONE,i.ZERO),i.blendFuncSeparate(i.ONE,i.ZERO,i.ONE,i.ZERO),i.blendColor(0,0,0,0),i.colorMask(!0,!0,!0,!0),i.clearColor(0,0,0,0),i.depthMask(!0),i.depthFunc(i.LESS),a.setReversed(!1),i.clearDepth(1),i.stencilMask(4294967295),i.stencilFunc(i.ALWAYS,0,4294967295),i.stencilOp(i.KEEP,i.KEEP,i.KEEP),i.clearStencil(0),i.cullFace(i.BACK),i.frontFace(i.CCW),i.polygonOffset(0,0),i.activeTexture(i.TEXTURE0),i.bindFramebuffer(i.FRAMEBUFFER,null),i.bindFramebuffer(i.DRAW_FRAMEBUFFER,null),i.bindFramebuffer(i.READ_FRAMEBUFFER,null),i.useProgram(null),i.lineWidth(1),i.scissor(0,0,i.canvas.width,i.canvas.height),i.viewport(0,0,i.canvas.width,i.canvas.height),i.pixelStorei(i.PACK_ALIGNMENT,4),i.pixelStorei(i.UNPACK_ALIGNMENT,4),i.pixelStorei(i.UNPACK_FLIP_Y_WEBGL,!1),i.pixelStorei(i.UNPACK_PREMULTIPLY_ALPHA_WEBGL,!1),i.pixelStorei(i.UNPACK_COLORSPACE_CONVERSION_WEBGL,i.BROWSER_DEFAULT_WEBGL),i.pixelStorei(i.PACK_ROW_LENGTH,0),i.pixelStorei(i.PACK_SKIP_PIXELS,0),i.pixelStorei(i.PACK_SKIP_ROWS,0),i.pixelStorei(i.UNPACK_ROW_LENGTH,0),i.pixelStorei(i.UNPACK_IMAGE_HEIGHT,0),i.pixelStorei(i.UNPACK_SKIP_PIXELS,0),i.pixelStorei(i.UNPACK_SKIP_ROWS,0),i.pixelStorei(i.UNPACK_SKIP_IMAGES,0),d={},p={},j=null,et={},h={},_=new WeakMap,v=[],M=null,m=!1,u=null,A=null,P=null,S=null,b=null,y=null,w=null,x=new Gt(0,0,0),T=0,L=!1,F=null,G=null,k=null,U=null,H=null,ne.set(0,0,i.canvas.width,i.canvas.height),Vt.set(0,0,i.canvas.width,i.canvas.height),s.reset(),a.reset(),o.reset()}return{buffers:{color:s,depth:a,stencil:o},enable:Q,disable:gt,bindFramebuffer:Pt,drawBuffers:mt,useProgram:Ft,setBlending:Xt,setMaterial:ie,setFlipSided:zt,setCullFace:oe,setLineWidth:xe,setPolygonOffset:Le,setScissorTest:le,activeTexture:ue,bindTexture:D,unbindTexture:Ee,compressedTexImage2D:Kt,compressedTexImage3D:E,texImage2D:q,texImage3D:J,pixelStorei:Et,getParameter:st,updateUBOMapping:yt,uniformBlockBinding:wt,texStorage2D:it,texStorage3D:rt,texSubImage2D:f,texSubImage3D:N,compressedTexSubImage2D:z,compressedTexSubImage3D:W,scissor:ct,viewport:at,reset:It}}function ym(i,t,e,n,r,s,a){const o=t.has("WEBGL_multisampled_render_to_texture")?t.get("WEBGL_multisampled_render_to_texture"):null,c=typeof navigator>"u"?!1:/OculusBrowser/g.test(navigator.userAgent),l=new Ct,d=new WeakMap,p=new Set;let h;const _=new WeakMap;let v=!1;try{v=typeof OffscreenCanvas<"u"&&new OffscreenCanvas(1,1).getContext("2d")!==null}catch{}function M(E,f){return v?new OffscreenCanvas(E,f):Lr("canvas")}function m(E,f,N){let z=1;const W=Kt(E);if((W.width>N||W.height>N)&&(z=N/Math.max(W.width,W.height)),z<1)if(typeof HTMLImageElement<"u"&&E instanceof HTMLImageElement||typeof HTMLCanvasElement<"u"&&E instanceof HTMLCanvasElement||typeof ImageBitmap<"u"&&E instanceof ImageBitmap||typeof VideoFrame<"u"&&E instanceof VideoFrame){const it=Math.floor(z*W.width),rt=Math.floor(z*W.height);h===void 0&&(h=M(it,rt));const q=f?M(it,rt):h;return q.width=it,q.height=rt,q.getContext("2d").drawImage(E,0,0,it,rt),Rt("WebGLRenderer: Texture has been resized from ("+W.width+"x"+W.height+") to ("+it+"x"+rt+")."),q}else return"data"in E&&Rt("WebGLRenderer: Image in DataTexture is too big ("+W.width+"x"+W.height+")."),E;return E}function u(E){return E.generateMipmaps}function A(E){i.generateMipmap(E)}function P(E){return E.isWebGLCubeRenderTarget?i.TEXTURE_CUBE_MAP:E.isWebGL3DRenderTarget?i.TEXTURE_3D:E.isWebGLArrayRenderTarget||E.isCompressedArrayTexture?i.TEXTURE_2D_ARRAY:i.TEXTURE_2D}function S(E,f,N,z,W,it=!1){if(E!==null){if(i[E]!==void 0)return i[E];Rt("WebGLRenderer: Attempt to use non-existing WebGL internal format '"+E+"'")}let rt;z&&(rt=t.get("EXT_texture_norm16"),rt||Rt("WebGLRenderer: Unable to use normalized textures without EXT_texture_norm16 extension"));let q=f;if(f===i.RED&&(N===i.FLOAT&&(q=i.R32F),N===i.HALF_FLOAT&&(q=i.R16F),N===i.UNSIGNED_BYTE&&(q=i.R8),N===i.UNSIGNED_SHORT&&rt&&(q=rt.R16_EXT),N===i.SHORT&&rt&&(q=rt.R16_SNORM_EXT)),f===i.RED_INTEGER&&(N===i.UNSIGNED_BYTE&&(q=i.R8UI),N===i.UNSIGNED_SHORT&&(q=i.R16UI),N===i.UNSIGNED_INT&&(q=i.R32UI),N===i.BYTE&&(q=i.R8I),N===i.SHORT&&(q=i.R16I),N===i.INT&&(q=i.R32I)),f===i.RG&&(N===i.FLOAT&&(q=i.RG32F),N===i.HALF_FLOAT&&(q=i.RG16F),N===i.UNSIGNED_BYTE&&(q=i.RG8),N===i.UNSIGNED_SHORT&&rt&&(q=rt.RG16_EXT),N===i.SHORT&&rt&&(q=rt.RG16_SNORM_EXT)),f===i.RG_INTEGER&&(N===i.UNSIGNED_BYTE&&(q=i.RG8UI),N===i.UNSIGNED_SHORT&&(q=i.RG16UI),N===i.UNSIGNED_INT&&(q=i.RG32UI),N===i.BYTE&&(q=i.RG8I),N===i.SHORT&&(q=i.RG16I),N===i.INT&&(q=i.RG32I)),f===i.RGB_INTEGER&&(N===i.UNSIGNED_BYTE&&(q=i.RGB8UI),N===i.UNSIGNED_SHORT&&(q=i.RGB16UI),N===i.UNSIGNED_INT&&(q=i.RGB32UI),N===i.BYTE&&(q=i.RGB8I),N===i.SHORT&&(q=i.RGB16I),N===i.INT&&(q=i.RGB32I)),f===i.RGBA_INTEGER&&(N===i.UNSIGNED_BYTE&&(q=i.RGBA8UI),N===i.UNSIGNED_SHORT&&(q=i.RGBA16UI),N===i.UNSIGNED_INT&&(q=i.RGBA32UI),N===i.BYTE&&(q=i.RGBA8I),N===i.SHORT&&(q=i.RGBA16I),N===i.INT&&(q=i.RGBA32I)),f===i.RGB&&(N===i.UNSIGNED_SHORT&&rt&&(q=rt.RGB16_EXT),N===i.SHORT&&rt&&(q=rt.RGB16_SNORM_EXT),N===i.UNSIGNED_INT_5_9_9_9_REV&&(q=i.RGB9_E5),N===i.UNSIGNED_INT_10F_11F_11F_REV&&(q=i.R11F_G11F_B10F)),f===i.RGBA){const J=it?Dr:Ht.getTransfer(W);N===i.FLOAT&&(q=i.RGBA32F),N===i.HALF_FLOAT&&(q=i.RGBA16F),N===i.UNSIGNED_BYTE&&(q=J===$t?i.SRGB8_ALPHA8:i.RGBA8),N===i.UNSIGNED_SHORT&&rt&&(q=rt.RGBA16_EXT),N===i.SHORT&&rt&&(q=rt.RGBA16_SNORM_EXT),N===i.UNSIGNED_SHORT_4_4_4_4&&(q=i.RGBA4),N===i.UNSIGNED_SHORT_5_5_5_1&&(q=i.RGB5_A1)}return(q===i.R16F||q===i.R32F||q===i.RG16F||q===i.RG32F||q===i.RGBA16F||q===i.RGBA32F)&&t.get("EXT_color_buffer_float"),q}function b(E,f){let N;return E?f===null||f===ln||f===Bi?N=i.DEPTH24_STENCIL8:f===Ke?N=i.DEPTH32F_STENCIL8:f===Oi&&(N=i.DEPTH24_STENCIL8,Rt("DepthTexture: 16 bit depth attachment is not supported with stencil. Using 24-bit attachment.")):f===null||f===ln||f===Bi?N=i.DEPTH_COMPONENT24:f===Ke?N=i.DEPTH_COMPONENT32F:f===Oi&&(N=i.DEPTH_COMPONENT16),N}function y(E,f){return u(E)===!0||E.isFramebufferTexture&&E.minFilter!==Me&&E.minFilter!==Ae?Math.log2(Math.max(f.width,f.height))+1:E.mipmaps!==void 0&&E.mipmaps.length>0?E.mipmaps.length:E.isCompressedTexture&&Array.isArray(E.image)?f.mipmaps.length:1}function w(E){const f=E.target;f.removeEventListener("dispose",w),T(f),f.isVideoTexture&&d.delete(f),f.isHTMLTexture&&p.delete(f)}function x(E){const f=E.target;f.removeEventListener("dispose",x),F(f)}function T(E){const f=n.get(E);if(f.__webglInit===void 0)return;const N=E.source,z=_.get(N);if(z){const W=z[f.__cacheKey];W.usedTimes--,W.usedTimes===0&&L(E),Object.keys(z).length===0&&_.delete(N)}n.remove(E)}function L(E){const f=n.get(E);i.deleteTexture(f.__webglTexture);const N=E.source,z=_.get(N);delete z[f.__cacheKey],a.memory.textures--}function F(E){const f=n.get(E);if(E.depthTexture&&(E.depthTexture.dispose(),n.remove(E.depthTexture)),E.isWebGLCubeRenderTarget)for(let z=0;z<6;z++){if(Array.isArray(f.__webglFramebuffer[z]))for(let W=0;W<f.__webglFramebuffer[z].length;W++)i.deleteFramebuffer(f.__webglFramebuffer[z][W]);else i.deleteFramebuffer(f.__webglFramebuffer[z]);f.__webglDepthbuffer&&i.deleteRenderbuffer(f.__webglDepthbuffer[z])}else{if(Array.isArray(f.__webglFramebuffer))for(let z=0;z<f.__webglFramebuffer.length;z++)i.deleteFramebuffer(f.__webglFramebuffer[z]);else i.deleteFramebuffer(f.__webglFramebuffer);if(f.__webglDepthbuffer&&i.deleteRenderbuffer(f.__webglDepthbuffer),f.__webglMultisampledFramebuffer&&i.deleteFramebuffer(f.__webglMultisampledFramebuffer),f.__webglColorRenderbuffer)for(let z=0;z<f.__webglColorRenderbuffer.length;z++)f.__webglColorRenderbuffer[z]&&i.deleteRenderbuffer(f.__webglColorRenderbuffer[z]);f.__webglDepthRenderbuffer&&i.deleteRenderbuffer(f.__webglDepthRenderbuffer)}const N=E.textures;for(let z=0,W=N.length;z<W;z++){const it=n.get(N[z]);it.__webglTexture&&(i.deleteTexture(it.__webglTexture),a.memory.textures--),n.remove(N[z])}n.remove(E)}let G=0;function k(){G=0}function U(){return G}function H(E){G=E}function K(){const E=G;return E>=r.maxTextures&&Rt("WebGLTextures: Trying to use "+(E+1)+" texture units while this GPU supports only "+r.maxTextures),G+=1,E}function Z(E){const f=[];return f.push(E.wrapS),f.push(E.wrapT),f.push(E.wrapR||0),f.push(E.magFilter),f.push(E.minFilter),f.push(E.anisotropy),f.push(E.internalFormat),f.push(E.format),f.push(E.type),f.push(E.generateMipmaps),f.push(E.premultiplyAlpha),f.push(E.flipY),f.push(E.unpackAlignment),f.push(E.colorSpace),f.join()}function nt(E,f){const N=n.get(E);if(E.isVideoTexture&&D(E),E.isRenderTargetTexture===!1&&E.isExternalTexture!==!0&&E.version>0&&N.__version!==E.version){const z=E.image;if(z===null)Rt("WebGLRenderer: Texture marked for update but no image data found.");else if(z.complete===!1)Rt("WebGLRenderer: Texture marked for update but image is incomplete");else{gt(N,E,f);return}}else E.isExternalTexture&&(N.__webglTexture=E.sourceTexture?E.sourceTexture:null);e.bindTexture(i.TEXTURE_2D,N.__webglTexture,i.TEXTURE0+f)}function X(E,f){const N=n.get(E);if(E.isRenderTargetTexture===!1&&E.version>0&&N.__version!==E.version){gt(N,E,f);return}else E.isExternalTexture&&(N.__webglTexture=E.sourceTexture?E.sourceTexture:null);e.bindTexture(i.TEXTURE_2D_ARRAY,N.__webglTexture,i.TEXTURE0+f)}function j(E,f){const N=n.get(E);if(E.isRenderTargetTexture===!1&&E.version>0&&N.__version!==E.version){gt(N,E,f);return}e.bindTexture(i.TEXTURE_3D,N.__webglTexture,i.TEXTURE0+f)}function et(E,f){const N=n.get(E);if(E.isCubeDepthTexture!==!0&&E.version>0&&N.__version!==E.version){Pt(N,E,f);return}e.bindTexture(i.TEXTURE_CUBE_MAP,N.__webglTexture,i.TEXTURE0+f)}const At={[Ls]:i.REPEAT,[xn]:i.CLAMP_TO_EDGE,[Is]:i.MIRRORED_REPEAT},bt={[Me]:i.NEAREST,[vc]:i.NEAREST_MIPMAP_NEAREST,[Xi]:i.NEAREST_MIPMAP_LINEAR,[Ae]:i.LINEAR,[Yr]:i.LINEAR_MIPMAP_NEAREST,[Hn]:i.LINEAR_MIPMAP_LINEAR},ne={[yc]:i.NEVER,[Rc]:i.ALWAYS,[bc]:i.LESS,[Ma]:i.LEQUAL,[Tc]:i.EQUAL,[Ea]:i.GEQUAL,[Ac]:i.GREATER,[wc]:i.NOTEQUAL};function Vt(E,f){if(f.type===Ke&&t.has("OES_texture_float_linear")===!1&&(f.magFilter===Ae||f.magFilter===Yr||f.magFilter===Xi||f.magFilter===Hn||f.minFilter===Ae||f.minFilter===Yr||f.minFilter===Xi||f.minFilter===Hn)&&Rt("WebGLRenderer: Unable to use linear filtering with floating point textures. OES_texture_float_linear not supported on this device."),i.texParameteri(E,i.TEXTURE_WRAP_S,At[f.wrapS]),i.texParameteri(E,i.TEXTURE_WRAP_T,At[f.wrapT]),(E===i.TEXTURE_3D||E===i.TEXTURE_2D_ARRAY)&&i.texParameteri(E,i.TEXTURE_WRAP_R,At[f.wrapR]),i.texParameteri(E,i.TEXTURE_MAG_FILTER,bt[f.magFilter]),i.texParameteri(E,i.TEXTURE_MIN_FILTER,bt[f.minFilter]),f.compareFunction&&(i.texParameteri(E,i.TEXTURE_COMPARE_MODE,i.COMPARE_REF_TO_TEXTURE),i.texParameteri(E,i.TEXTURE_COMPARE_FUNC,ne[f.compareFunction])),t.has("EXT_texture_filter_anisotropic")===!0){if(f.magFilter===Me||f.minFilter!==Xi&&f.minFilter!==Hn||f.type===Ke&&t.has("OES_texture_float_linear")===!1)return;if(f.anisotropy>1||n.get(f).__currentAnisotropy){const N=t.get("EXT_texture_filter_anisotropic");i.texParameterf(E,N.TEXTURE_MAX_ANISOTROPY_EXT,Math.min(f.anisotropy,r.getMaxAnisotropy())),n.get(f).__currentAnisotropy=f.anisotropy}}}function Yt(E,f){let N=!1;E.__webglInit===void 0&&(E.__webglInit=!0,f.addEventListener("dispose",w));const z=f.source;let W=_.get(z);W===void 0&&(W={},_.set(z,W));const it=Z(f);if(it!==E.__cacheKey){W[it]===void 0&&(W[it]={texture:i.createTexture(),usedTimes:0},a.memory.textures++,N=!0),W[it].usedTimes++;const rt=W[E.__cacheKey];rt!==void 0&&(W[E.__cacheKey].usedTimes--,rt.usedTimes===0&&L(f)),E.__cacheKey=it,E.__webglTexture=W[it].texture}return N}function Y(E,f,N){return Math.floor(Math.floor(E/N)/f)}function Q(E,f,N,z){const it=E.updateRanges;if(it.length===0)e.texSubImage2D(i.TEXTURE_2D,0,0,0,f.width,f.height,N,z,f.data);else{it.sort((Et,ct)=>Et.start-ct.start);let rt=0;for(let Et=1;Et<it.length;Et++){const ct=it[rt],at=it[Et],yt=ct.start+ct.count,wt=Y(at.start,f.width,4),It=Y(ct.start,f.width,4);at.start<=yt+1&&wt===It&&Y(at.start+at.count-1,f.width,4)===wt?ct.count=Math.max(ct.count,at.start+at.count-ct.start):(++rt,it[rt]=at)}it.length=rt+1;const q=e.getParameter(i.UNPACK_ROW_LENGTH),J=e.getParameter(i.UNPACK_SKIP_PIXELS),st=e.getParameter(i.UNPACK_SKIP_ROWS);e.pixelStorei(i.UNPACK_ROW_LENGTH,f.width);for(let Et=0,ct=it.length;Et<ct;Et++){const at=it[Et],yt=Math.floor(at.start/4),wt=Math.ceil(at.count/4),It=yt%f.width,C=Math.floor(yt/f.width),ot=wt,$=1;e.pixelStorei(i.UNPACK_SKIP_PIXELS,It),e.pixelStorei(i.UNPACK_SKIP_ROWS,C),e.texSubImage2D(i.TEXTURE_2D,0,It,C,ot,$,N,z,f.data)}E.clearUpdateRanges(),e.pixelStorei(i.UNPACK_ROW_LENGTH,q),e.pixelStorei(i.UNPACK_SKIP_PIXELS,J),e.pixelStorei(i.UNPACK_SKIP_ROWS,st)}}function gt(E,f,N){let z=i.TEXTURE_2D;(f.isDataArrayTexture||f.isCompressedArrayTexture)&&(z=i.TEXTURE_2D_ARRAY),f.isData3DTexture&&(z=i.TEXTURE_3D);const W=Yt(E,f),it=f.source;e.bindTexture(z,E.__webglTexture,i.TEXTURE0+N);const rt=n.get(it);if(it.version!==rt.__version||W===!0){if(e.activeTexture(i.TEXTURE0+N),(typeof ImageBitmap<"u"&&f.image instanceof ImageBitmap)===!1){const $=Ht.getPrimaries(Ht.workingColorSpace),lt=f.colorSpace===Dn?null:Ht.getPrimaries(f.colorSpace),ft=f.colorSpace===Dn||$===lt?i.NONE:i.BROWSER_DEFAULT_WEBGL;e.pixelStorei(i.UNPACK_FLIP_Y_WEBGL,f.flipY),e.pixelStorei(i.UNPACK_PREMULTIPLY_ALPHA_WEBGL,f.premultiplyAlpha),e.pixelStorei(i.UNPACK_COLORSPACE_CONVERSION_WEBGL,ft)}e.pixelStorei(i.UNPACK_ALIGNMENT,f.unpackAlignment);let J=m(f.image,!1,r.maxTextureSize);J=Ee(f,J);const st=s.convert(f.format,f.colorSpace),Et=s.convert(f.type);let ct=S(f.internalFormat,st,Et,f.normalized,f.colorSpace,f.isVideoTexture);Vt(z,f);let at;const yt=f.mipmaps,wt=f.isVideoTexture!==!0,It=rt.__version===void 0||W===!0,C=it.dataReady,ot=y(f,J);if(f.isDepthTexture)ct=b(f.format===Vn,f.type),It&&(wt?e.texStorage2D(i.TEXTURE_2D,1,ct,J.width,J.height):e.texImage2D(i.TEXTURE_2D,0,ct,J.width,J.height,0,st,Et,null));else if(f.isDataTexture)if(yt.length>0){wt&&It&&e.texStorage2D(i.TEXTURE_2D,ot,ct,yt[0].width,yt[0].height);for(let $=0,lt=yt.length;$<lt;$++)at=yt[$],wt?C&&e.texSubImage2D(i.TEXTURE_2D,$,0,0,at.width,at.height,st,Et,at.data):e.texImage2D(i.TEXTURE_2D,$,ct,at.width,at.height,0,st,Et,at.data);f.generateMipmaps=!1}else wt?(It&&e.texStorage2D(i.TEXTURE_2D,ot,ct,J.width,J.height),C&&Q(f,J,st,Et)):e.texImage2D(i.TEXTURE_2D,0,ct,J.width,J.height,0,st,Et,J.data);else if(f.isCompressedTexture)if(f.isCompressedArrayTexture){wt&&It&&e.texStorage3D(i.TEXTURE_2D_ARRAY,ot,ct,yt[0].width,yt[0].height,J.depth);for(let $=0,lt=yt.length;$<lt;$++)if(at=yt[$],f.format!==$e)if(st!==null)if(wt){if(C)if(f.layerUpdates.size>0){const ft=Do(at.width,at.height,f.format,f.type);for(const tt of f.layerUpdates){const Tt=at.data.subarray(tt*ft/at.data.BYTES_PER_ELEMENT,(tt+1)*ft/at.data.BYTES_PER_ELEMENT);e.compressedTexSubImage3D(i.TEXTURE_2D_ARRAY,$,0,0,tt,at.width,at.height,1,st,Tt)}}else e.compressedTexSubImage3D(i.TEXTURE_2D_ARRAY,$,0,0,0,at.width,at.height,J.depth,st,at.data)}else e.compressedTexImage3D(i.TEXTURE_2D_ARRAY,$,ct,at.width,at.height,J.depth,0,at.data,0,0);else Rt("WebGLRenderer: Attempt to load unsupported compressed texture format in .uploadTexture()");else wt?C&&e.texSubImage3D(i.TEXTURE_2D_ARRAY,$,0,0,0,at.width,at.height,J.depth,st,Et,at.data):e.texImage3D(i.TEXTURE_2D_ARRAY,$,ct,at.width,at.height,J.depth,0,st,Et,at.data);f.layerUpdates.size>0&&f.clearLayerUpdates()}else{wt&&It&&e.texStorage2D(i.TEXTURE_2D,ot,ct,yt[0].width,yt[0].height);for(let $=0,lt=yt.length;$<lt;$++)at=yt[$],f.format!==$e?st!==null?wt?C&&e.compressedTexSubImage2D(i.TEXTURE_2D,$,0,0,at.width,at.height,st,at.data):e.compressedTexImage2D(i.TEXTURE_2D,$,ct,at.width,at.height,0,at.data):Rt("WebGLRenderer: Attempt to load unsupported compressed texture format in .uploadTexture()"):wt?C&&e.texSubImage2D(i.TEXTURE_2D,$,0,0,at.width,at.height,st,Et,at.data):e.texImage2D(i.TEXTURE_2D,$,ct,at.width,at.height,0,st,Et,at.data)}else if(f.isDataArrayTexture)if(wt){if(It&&e.texStorage3D(i.TEXTURE_2D_ARRAY,ot,ct,J.width,J.height,J.depth),C)if(f.layerUpdates.size>0){const $=Do(J.width,J.height,f.format,f.type);for(const lt of f.layerUpdates){const ft=J.data.subarray(lt*$/J.data.BYTES_PER_ELEMENT,(lt+1)*$/J.data.BYTES_PER_ELEMENT);e.texSubImage3D(i.TEXTURE_2D_ARRAY,0,0,0,lt,J.width,J.height,1,st,Et,ft)}f.clearLayerUpdates()}else e.texSubImage3D(i.TEXTURE_2D_ARRAY,0,0,0,0,J.width,J.height,J.depth,st,Et,J.data)}else e.texImage3D(i.TEXTURE_2D_ARRAY,0,ct,J.width,J.height,J.depth,0,st,Et,J.data);else if(f.isData3DTexture)wt?(It&&e.texStorage3D(i.TEXTURE_3D,ot,ct,J.width,J.height,J.depth),C&&e.texSubImage3D(i.TEXTURE_3D,0,0,0,0,J.width,J.height,J.depth,st,Et,J.data)):e.texImage3D(i.TEXTURE_3D,0,ct,J.width,J.height,J.depth,0,st,Et,J.data);else if(f.isFramebufferTexture){if(It)if(wt)e.texStorage2D(i.TEXTURE_2D,ot,ct,J.width,J.height);else{let $=J.width,lt=J.height;for(let ft=0;ft<ot;ft++)e.texImage2D(i.TEXTURE_2D,ft,ct,$,lt,0,st,Et,null),$>>=1,lt>>=1}}else if(f.isHTMLTexture){if("texElementImage2D"in i){const $=i.canvas;if($.hasAttribute("layoutsubtree")||$.setAttribute("layoutsubtree","true"),J.parentNode!==$){$.appendChild(J),p.add(f),$.onpaint=lt=>{const ft=lt.changedElements;for(const tt of p)ft.includes(tt.image)&&(tt.needsUpdate=!0)},$.requestPaint();return}if(i.texElementImage2D.length===3)i.texElementImage2D(i.TEXTURE_2D,i.RGBA8,J);else{const ft=i.RGBA,tt=i.RGBA,Tt=i.UNSIGNED_BYTE;i.texElementImage2D(i.TEXTURE_2D,0,ft,tt,Tt,J)}i.texParameteri(i.TEXTURE_2D,i.TEXTURE_MIN_FILTER,i.LINEAR),i.texParameteri(i.TEXTURE_2D,i.TEXTURE_WRAP_S,i.CLAMP_TO_EDGE),i.texParameteri(i.TEXTURE_2D,i.TEXTURE_WRAP_T,i.CLAMP_TO_EDGE)}}else if(yt.length>0){if(wt&&It){const $=Kt(yt[0]);e.texStorage2D(i.TEXTURE_2D,ot,ct,$.width,$.height)}for(let $=0,lt=yt.length;$<lt;$++)at=yt[$],wt?C&&e.texSubImage2D(i.TEXTURE_2D,$,0,0,st,Et,at):e.texImage2D(i.TEXTURE_2D,$,ct,st,Et,at);f.generateMipmaps=!1}else if(wt){if(It){const $=Kt(J);e.texStorage2D(i.TEXTURE_2D,ot,ct,$.width,$.height)}C&&e.texSubImage2D(i.TEXTURE_2D,0,0,0,st,Et,J)}else e.texImage2D(i.TEXTURE_2D,0,ct,st,Et,J);u(f)&&A(z),rt.__version=it.version,f.onUpdate&&f.onUpdate(f)}E.__version=f.version}function Pt(E,f,N){if(f.image.length!==6)return;const z=Yt(E,f),W=f.source;e.bindTexture(i.TEXTURE_CUBE_MAP,E.__webglTexture,i.TEXTURE0+N);const it=n.get(W);if(W.version!==it.__version||z===!0){e.activeTexture(i.TEXTURE0+N);const rt=Ht.getPrimaries(Ht.workingColorSpace),q=f.colorSpace===Dn?null:Ht.getPrimaries(f.colorSpace),J=f.colorSpace===Dn||rt===q?i.NONE:i.BROWSER_DEFAULT_WEBGL;e.pixelStorei(i.UNPACK_FLIP_Y_WEBGL,f.flipY),e.pixelStorei(i.UNPACK_PREMULTIPLY_ALPHA_WEBGL,f.premultiplyAlpha),e.pixelStorei(i.UNPACK_ALIGNMENT,f.unpackAlignment),e.pixelStorei(i.UNPACK_COLORSPACE_CONVERSION_WEBGL,J);const st=f.isCompressedTexture||f.image[0].isCompressedTexture,Et=f.image[0]&&f.image[0].isDataTexture,ct=[];for(let tt=0;tt<6;tt++)!st&&!Et?ct[tt]=m(f.image[tt],!0,r.maxCubemapSize):ct[tt]=Et?f.image[tt].image:f.image[tt],ct[tt]=Ee(f,ct[tt]);const at=ct[0],yt=s.convert(f.format,f.colorSpace),wt=s.convert(f.type),It=S(f.internalFormat,yt,wt,f.normalized,f.colorSpace),C=f.isVideoTexture!==!0,ot=it.__version===void 0||z===!0,$=W.dataReady;let lt=y(f,at);Vt(i.TEXTURE_CUBE_MAP,f);let ft;if(st){C&&ot&&e.texStorage2D(i.TEXTURE_CUBE_MAP,lt,It,at.width,at.height);for(let tt=0;tt<6;tt++){ft=ct[tt].mipmaps;for(let Tt=0;Tt<ft.length;Tt++){const St=ft[Tt];f.format!==$e?yt!==null?C?$&&e.compressedTexSubImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,Tt,0,0,St.width,St.height,yt,St.data):e.compressedTexImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,Tt,It,St.width,St.height,0,St.data):Rt("WebGLRenderer: Attempt to load unsupported compressed texture format in .setTextureCube()"):C?$&&e.texSubImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,Tt,0,0,St.width,St.height,yt,wt,St.data):e.texImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,Tt,It,St.width,St.height,0,yt,wt,St.data)}}}else{if(ft=f.mipmaps,C&&ot){ft.length>0&&lt++;const tt=Kt(ct[0]);e.texStorage2D(i.TEXTURE_CUBE_MAP,lt,It,tt.width,tt.height)}for(let tt=0;tt<6;tt++)if(Et){C?$&&e.texSubImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,0,0,0,ct[tt].width,ct[tt].height,yt,wt,ct[tt].data):e.texImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,0,It,ct[tt].width,ct[tt].height,0,yt,wt,ct[tt].data);for(let Tt=0;Tt<ft.length;Tt++){const re=ft[Tt].image[tt].image;C?$&&e.texSubImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,Tt+1,0,0,re.width,re.height,yt,wt,re.data):e.texImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,Tt+1,It,re.width,re.height,0,yt,wt,re.data)}}else{C?$&&e.texSubImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,0,0,0,yt,wt,ct[tt]):e.texImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,0,It,yt,wt,ct[tt]);for(let Tt=0;Tt<ft.length;Tt++){const St=ft[Tt];C?$&&e.texSubImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,Tt+1,0,0,yt,wt,St.image[tt]):e.texImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+tt,Tt+1,It,yt,wt,St.image[tt])}}}u(f)&&A(i.TEXTURE_CUBE_MAP),it.__version=W.version,f.onUpdate&&f.onUpdate(f)}E.__version=f.version}function mt(E,f,N,z,W,it){const rt=s.convert(N.format,N.colorSpace),q=s.convert(N.type),J=S(N.internalFormat,rt,q,N.normalized,N.colorSpace),st=n.get(f),Et=n.get(N);if(Et.__renderTarget=f,!st.__hasExternalTextures){const ct=Math.max(1,f.width>>it),at=Math.max(1,f.height>>it);W===i.TEXTURE_3D||W===i.TEXTURE_2D_ARRAY?e.texImage3D(W,it,J,ct,at,f.depth,0,rt,q,null):e.texImage2D(W,it,J,ct,at,0,rt,q,null)}e.bindFramebuffer(i.FRAMEBUFFER,E),ue(f)?o.framebufferTexture2DMultisampleEXT(i.FRAMEBUFFER,z,W,Et.__webglTexture,0,le(f)):(W===i.TEXTURE_2D||W>=i.TEXTURE_CUBE_MAP_POSITIVE_X&&W<=i.TEXTURE_CUBE_MAP_NEGATIVE_Z)&&i.framebufferTexture2D(i.FRAMEBUFFER,z,W,Et.__webglTexture,it),e.bindFramebuffer(i.FRAMEBUFFER,null)}function Ft(E,f,N){if(i.bindRenderbuffer(i.RENDERBUFFER,E),f.depthBuffer){const z=f.depthTexture,W=z&&z.isDepthTexture?z.type:null,it=b(f.stencilBuffer,W),rt=f.stencilBuffer?i.DEPTH_STENCIL_ATTACHMENT:i.DEPTH_ATTACHMENT;ue(f)?o.renderbufferStorageMultisampleEXT(i.RENDERBUFFER,le(f),it,f.width,f.height):N?i.renderbufferStorageMultisample(i.RENDERBUFFER,le(f),it,f.width,f.height):i.renderbufferStorage(i.RENDERBUFFER,it,f.width,f.height),i.framebufferRenderbuffer(i.FRAMEBUFFER,rt,i.RENDERBUFFER,E)}else{const z=f.textures;for(let W=0;W<z.length;W++){const it=z[W],rt=s.convert(it.format,it.colorSpace),q=s.convert(it.type),J=S(it.internalFormat,rt,q,it.normalized,it.colorSpace);ue(f)?o.renderbufferStorageMultisampleEXT(i.RENDERBUFFER,le(f),J,f.width,f.height):N?i.renderbufferStorageMultisample(i.RENDERBUFFER,le(f),J,f.width,f.height):i.renderbufferStorage(i.RENDERBUFFER,J,f.width,f.height)}}i.bindRenderbuffer(i.RENDERBUFFER,null)}function pe(E,f,N){const z=f.isWebGLCubeRenderTarget===!0;if(e.bindFramebuffer(i.FRAMEBUFFER,E),!(f.depthTexture&&f.depthTexture.isDepthTexture))throw new Error("THREE.WebGLTextures: renderTarget.depthTexture must be an instance of THREE.DepthTexture.");const W=n.get(f.depthTexture);if(W.__renderTarget=f,(!W.__webglTexture||f.depthTexture.image.width!==f.width||f.depthTexture.image.height!==f.height)&&(f.depthTexture.image.width=f.width,f.depthTexture.image.height=f.height,f.depthTexture.needsUpdate=!0),z){if(W.__webglInit===void 0&&(W.__webglInit=!0,f.depthTexture.addEventListener("dispose",w)),W.__webglTexture===void 0){W.__webglTexture=i.createTexture(),e.bindTexture(i.TEXTURE_CUBE_MAP,W.__webglTexture),Vt(i.TEXTURE_CUBE_MAP,f.depthTexture);const st=s.convert(f.depthTexture.format),Et=s.convert(f.depthTexture.type);let ct;f.depthTexture.format===En?ct=i.DEPTH_COMPONENT24:f.depthTexture.format===Vn&&(ct=i.DEPTH24_STENCIL8);for(let at=0;at<6;at++)i.texImage2D(i.TEXTURE_CUBE_MAP_POSITIVE_X+at,0,ct,f.width,f.height,0,st,Et,null)}}else nt(f.depthTexture,0);const it=W.__webglTexture,rt=le(f),q=z?i.TEXTURE_CUBE_MAP_POSITIVE_X+N:i.TEXTURE_2D,J=f.depthTexture.format===Vn?i.DEPTH_STENCIL_ATTACHMENT:i.DEPTH_ATTACHMENT;if(f.depthTexture.format===En)ue(f)?o.framebufferTexture2DMultisampleEXT(i.FRAMEBUFFER,J,q,it,0,rt):i.framebufferTexture2D(i.FRAMEBUFFER,J,q,it,0);else if(f.depthTexture.format===Vn)ue(f)?o.framebufferTexture2DMultisampleEXT(i.FRAMEBUFFER,J,q,it,0,rt):i.framebufferTexture2D(i.FRAMEBUFFER,J,q,it,0);else throw new Error("THREE.WebGLTextures: Unknown depthTexture format.")}function Ot(E){const f=n.get(E),N=E.isWebGLCubeRenderTarget===!0;if(f.__boundDepthTexture!==E.depthTexture){const z=E.depthTexture;if(f.__depthDisposeCallback&&f.__depthDisposeCallback(),z){const W=()=>{delete f.__boundDepthTexture,delete f.__depthDisposeCallback,z.removeEventListener("dispose",W)};z.addEventListener("dispose",W),f.__depthDisposeCallback=W}f.__boundDepthTexture=z}if(E.depthTexture&&!f.__autoAllocateDepthBuffer)if(N)for(let z=0;z<6;z++)pe(f.__webglFramebuffer[z],E,z);else{const z=E.texture.mipmaps;z&&z.length>0?pe(f.__webglFramebuffer[0],E,0):pe(f.__webglFramebuffer,E,0)}else if(N){f.__webglDepthbuffer=[];for(let z=0;z<6;z++)if(e.bindFramebuffer(i.FRAMEBUFFER,f.__webglFramebuffer[z]),f.__webglDepthbuffer[z]===void 0)f.__webglDepthbuffer[z]=i.createRenderbuffer(),Ft(f.__webglDepthbuffer[z],E,!1);else{const W=E.stencilBuffer?i.DEPTH_STENCIL_ATTACHMENT:i.DEPTH_ATTACHMENT,it=f.__webglDepthbuffer[z];i.bindRenderbuffer(i.RENDERBUFFER,it),i.framebufferRenderbuffer(i.FRAMEBUFFER,W,i.RENDERBUFFER,it)}}else{const z=E.texture.mipmaps;if(z&&z.length>0?e.bindFramebuffer(i.FRAMEBUFFER,f.__webglFramebuffer[0]):e.bindFramebuffer(i.FRAMEBUFFER,f.__webglFramebuffer),f.__webglDepthbuffer===void 0)f.__webglDepthbuffer=i.createRenderbuffer(),Ft(f.__webglDepthbuffer,E,!1);else{const W=E.stencilBuffer?i.DEPTH_STENCIL_ATTACHMENT:i.DEPTH_ATTACHMENT,it=f.__webglDepthbuffer;i.bindRenderbuffer(i.RENDERBUFFER,it),i.framebufferRenderbuffer(i.FRAMEBUFFER,W,i.RENDERBUFFER,it)}}e.bindFramebuffer(i.FRAMEBUFFER,null)}function Xt(E,f,N){const z=n.get(E);f!==void 0&&mt(z.__webglFramebuffer,E,E.texture,i.COLOR_ATTACHMENT0,i.TEXTURE_2D,0),N!==void 0&&Ot(E)}function ie(E){const f=E.texture,N=n.get(E),z=n.get(f);E.addEventListener("dispose",x);const W=E.textures,it=E.isWebGLCubeRenderTarget===!0,rt=W.length>1;if(rt||(z.__webglTexture===void 0&&(z.__webglTexture=i.createTexture()),z.__version=f.version,a.memory.textures++),it){N.__webglFramebuffer=[];for(let q=0;q<6;q++)if(f.mipmaps&&f.mipmaps.length>0){N.__webglFramebuffer[q]=[];for(let J=0;J<f.mipmaps.length;J++)N.__webglFramebuffer[q][J]=i.createFramebuffer()}else N.__webglFramebuffer[q]=i.createFramebuffer()}else{if(f.mipmaps&&f.mipmaps.length>0){N.__webglFramebuffer=[];for(let q=0;q<f.mipmaps.length;q++)N.__webglFramebuffer[q]=i.createFramebuffer()}else N.__webglFramebuffer=i.createFramebuffer();if(rt)for(let q=0,J=W.length;q<J;q++){const st=n.get(W[q]);st.__webglTexture===void 0&&(st.__webglTexture=i.createTexture(),a.memory.textures++)}if(E.samples>0&&ue(E)===!1){N.__webglMultisampledFramebuffer=i.createFramebuffer(),N.__webglColorRenderbuffer=[],e.bindFramebuffer(i.FRAMEBUFFER,N.__webglMultisampledFramebuffer);for(let q=0;q<W.length;q++){const J=W[q];N.__webglColorRenderbuffer[q]=i.createRenderbuffer(),i.bindRenderbuffer(i.RENDERBUFFER,N.__webglColorRenderbuffer[q]);const st=s.convert(J.format,J.colorSpace),Et=s.convert(J.type),ct=S(J.internalFormat,st,Et,J.normalized,J.colorSpace,E.isXRRenderTarget===!0),at=le(E);i.renderbufferStorageMultisample(i.RENDERBUFFER,at,ct,E.width,E.height),i.framebufferRenderbuffer(i.FRAMEBUFFER,i.COLOR_ATTACHMENT0+q,i.RENDERBUFFER,N.__webglColorRenderbuffer[q])}i.bindRenderbuffer(i.RENDERBUFFER,null),E.depthBuffer&&(N.__webglDepthRenderbuffer=i.createRenderbuffer(),Ft(N.__webglDepthRenderbuffer,E,!0)),e.bindFramebuffer(i.FRAMEBUFFER,null)}}if(it){e.bindTexture(i.TEXTURE_CUBE_MAP,z.__webglTexture),Vt(i.TEXTURE_CUBE_MAP,f);for(let q=0;q<6;q++)if(f.mipmaps&&f.mipmaps.length>0)for(let J=0;J<f.mipmaps.length;J++)mt(N.__webglFramebuffer[q][J],E,f,i.COLOR_ATTACHMENT0,i.TEXTURE_CUBE_MAP_POSITIVE_X+q,J);else mt(N.__webglFramebuffer[q],E,f,i.COLOR_ATTACHMENT0,i.TEXTURE_CUBE_MAP_POSITIVE_X+q,0);u(f)&&A(i.TEXTURE_CUBE_MAP),e.unbindTexture()}else if(rt){for(let q=0,J=W.length;q<J;q++){const st=W[q],Et=n.get(st);let ct=i.TEXTURE_2D;(E.isWebGL3DRenderTarget||E.isWebGLArrayRenderTarget)&&(ct=E.isWebGL3DRenderTarget?i.TEXTURE_3D:i.TEXTURE_2D_ARRAY),e.bindTexture(ct,Et.__webglTexture),Vt(ct,st),mt(N.__webglFramebuffer,E,st,i.COLOR_ATTACHMENT0+q,ct,0),u(st)&&A(ct)}e.unbindTexture()}else{let q=i.TEXTURE_2D;if((E.isWebGL3DRenderTarget||E.isWebGLArrayRenderTarget)&&(q=E.isWebGL3DRenderTarget?i.TEXTURE_3D:i.TEXTURE_2D_ARRAY),e.bindTexture(q,z.__webglTexture),Vt(q,f),f.mipmaps&&f.mipmaps.length>0)for(let J=0;J<f.mipmaps.length;J++)mt(N.__webglFramebuffer[J],E,f,i.COLOR_ATTACHMENT0,q,J);else mt(N.__webglFramebuffer,E,f,i.COLOR_ATTACHMENT0,q,0);u(f)&&A(q),e.unbindTexture()}E.depthBuffer&&Ot(E)}function zt(E){const f=E.textures;for(let N=0,z=f.length;N<z;N++){const W=f[N];if(u(W)){const it=P(E),rt=n.get(W).__webglTexture;e.bindTexture(it,rt),A(it),e.unbindTexture()}}}const oe=[],xe=[];function Le(E){if(E.samples>0){if(ue(E)===!1){const f=E.textures,N=E.width,z=E.height;let W=i.COLOR_BUFFER_BIT;const it=E.stencilBuffer?i.DEPTH_STENCIL_ATTACHMENT:i.DEPTH_ATTACHMENT,rt=n.get(E),q=f.length>1;if(q)for(let st=0;st<f.length;st++)e.bindFramebuffer(i.FRAMEBUFFER,rt.__webglMultisampledFramebuffer),i.framebufferRenderbuffer(i.FRAMEBUFFER,i.COLOR_ATTACHMENT0+st,i.RENDERBUFFER,null),e.bindFramebuffer(i.FRAMEBUFFER,rt.__webglFramebuffer),i.framebufferTexture2D(i.DRAW_FRAMEBUFFER,i.COLOR_ATTACHMENT0+st,i.TEXTURE_2D,null,0);e.bindFramebuffer(i.READ_FRAMEBUFFER,rt.__webglMultisampledFramebuffer);const J=E.texture.mipmaps;J&&J.length>0?e.bindFramebuffer(i.DRAW_FRAMEBUFFER,rt.__webglFramebuffer[0]):e.bindFramebuffer(i.DRAW_FRAMEBUFFER,rt.__webglFramebuffer);for(let st=0;st<f.length;st++){if(E.resolveDepthBuffer&&(E.depthBuffer&&(W|=i.DEPTH_BUFFER_BIT),E.stencilBuffer&&E.resolveStencilBuffer&&(W|=i.STENCIL_BUFFER_BIT)),q){i.framebufferRenderbuffer(i.READ_FRAMEBUFFER,i.COLOR_ATTACHMENT0,i.RENDERBUFFER,rt.__webglColorRenderbuffer[st]);const Et=n.get(f[st]).__webglTexture;i.framebufferTexture2D(i.DRAW_FRAMEBUFFER,i.COLOR_ATTACHMENT0,i.TEXTURE_2D,Et,0)}i.blitFramebuffer(0,0,N,z,0,0,N,z,W,i.NEAREST),c===!0&&(oe.length=0,xe.length=0,oe.push(i.COLOR_ATTACHMENT0+st),E.depthBuffer&&E.storeMultisampledDepthBuffer===!1&&(oe.push(it),xe.push(it),i.invalidateFramebuffer(i.DRAW_FRAMEBUFFER,xe)),i.invalidateFramebuffer(i.READ_FRAMEBUFFER,oe))}if(e.bindFramebuffer(i.READ_FRAMEBUFFER,null),e.bindFramebuffer(i.DRAW_FRAMEBUFFER,null),q)for(let st=0;st<f.length;st++){e.bindFramebuffer(i.FRAMEBUFFER,rt.__webglMultisampledFramebuffer),i.framebufferRenderbuffer(i.FRAMEBUFFER,i.COLOR_ATTACHMENT0+st,i.RENDERBUFFER,rt.__webglColorRenderbuffer[st]);const Et=n.get(f[st]).__webglTexture;e.bindFramebuffer(i.FRAMEBUFFER,rt.__webglFramebuffer),i.framebufferTexture2D(i.DRAW_FRAMEBUFFER,i.COLOR_ATTACHMENT0+st,i.TEXTURE_2D,Et,0)}e.bindFramebuffer(i.DRAW_FRAMEBUFFER,rt.__webglMultisampledFramebuffer)}else if(E.depthBuffer&&E.storeMultisampledDepthBuffer===!1&&c){const f=E.stencilBuffer?i.DEPTH_STENCIL_ATTACHMENT:i.DEPTH_ATTACHMENT;i.invalidateFramebuffer(i.DRAW_FRAMEBUFFER,[f])}}}function le(E){return Math.min(r.maxSamples,E.samples)}function ue(E){const f=n.get(E);return E.samples>0&&t.has("WEBGL_multisampled_render_to_texture")===!0&&f.__useRenderToTexture!==!1}function D(E){const f=a.render.frame;d.get(E)!==f&&(d.set(E,f),E.update())}function Ee(E,f){const N=E.colorSpace,z=E.format,W=E.type;return E.isCompressedTexture===!0||E.isVideoTexture===!0||N!==Pr&&N!==Dn&&(Ht.getTransfer(N)===$t?(z!==$e||W!==Be)&&Rt("WebGLTextures: sRGB encoded textures have to use RGBAFormat and UnsignedByteType."):Wt("WebGLTextures: Unsupported texture color space:",N)),f}function Kt(E){return typeof HTMLImageElement<"u"&&E instanceof HTMLImageElement?(l.width=E.naturalWidth||E.width,l.height=E.naturalHeight||E.height):typeof VideoFrame<"u"&&E instanceof VideoFrame?(l.width=E.displayWidth,l.height=E.displayHeight):(l.width=E.width,l.height=E.height),l}this.allocateTextureUnit=K,this.resetTextureUnits=k,this.getTextureUnits=U,this.setTextureUnits=H,this.setTexture2D=nt,this.setTexture2DArray=X,this.setTexture3D=j,this.setTextureCube=et,this.rebindTextures=Xt,this.setupRenderTarget=ie,this.updateRenderTargetMipmap=zt,this.updateMultisampleRenderTarget=Le,this.setupDepthRenderbuffer=Ot,this.setupFrameBufferTexture=mt,this.useMultisampledRTT=ue,this.isReversedDepthBuffer=function(){return e.buffers.depth.getReversed()}}function bm(i,t){function e(n,r=Dn){let s;const a=Ht.getTransfer(r);if(n===Be)return i.UNSIGNED_BYTE;if(n===ma)return i.UNSIGNED_SHORT_4_4_4_4;if(n===_a)return i.UNSIGNED_SHORT_5_5_5_1;if(n===ml)return i.UNSIGNED_INT_5_9_9_9_REV;if(n===_l)return i.UNSIGNED_INT_10F_11F_11F_REV;if(n===fl)return i.BYTE;if(n===pl)return i.SHORT;if(n===Oi)return i.UNSIGNED_SHORT;if(n===pa)return i.INT;if(n===ln)return i.UNSIGNED_INT;if(n===Ke)return i.FLOAT;if(n===cn)return i.HALF_FLOAT;if(n===gl)return i.ALPHA;if(n===xl)return i.RGB;if(n===$e)return i.RGBA;if(n===En)return i.DEPTH_COMPONENT;if(n===Vn)return i.DEPTH_STENCIL;if(n===ga)return i.RED;if(n===xa)return i.RED_INTEGER;if(n===Xn)return i.RG;if(n===va)return i.RG_INTEGER;if(n===Sa)return i.RGBA_INTEGER;if(n===yr||n===br||n===Tr||n===Ar)if(a===$t)if(s=t.get("WEBGL_compressed_texture_s3tc_srgb"),s!==null){if(n===yr)return s.COMPRESSED_SRGB_S3TC_DXT1_EXT;if(n===br)return s.COMPRESSED_SRGB_ALPHA_S3TC_DXT1_EXT;if(n===Tr)return s.COMPRESSED_SRGB_ALPHA_S3TC_DXT3_EXT;if(n===Ar)return s.COMPRESSED_SRGB_ALPHA_S3TC_DXT5_EXT}else return null;else if(s=t.get("WEBGL_compressed_texture_s3tc"),s!==null){if(n===yr)return s.COMPRESSED_RGB_S3TC_DXT1_EXT;if(n===br)return s.COMPRESSED_RGBA_S3TC_DXT1_EXT;if(n===Tr)return s.COMPRESSED_RGBA_S3TC_DXT3_EXT;if(n===Ar)return s.COMPRESSED_RGBA_S3TC_DXT5_EXT}else return null;if(n===Us||n===Ns||n===Fs||n===Os)if(s=t.get("WEBGL_compressed_texture_pvrtc"),s!==null){if(n===Us)return s.COMPRESSED_RGB_PVRTC_4BPPV1_IMG;if(n===Ns)return s.COMPRESSED_RGB_PVRTC_2BPPV1_IMG;if(n===Fs)return s.COMPRESSED_RGBA_PVRTC_4BPPV1_IMG;if(n===Os)return s.COMPRESSED_RGBA_PVRTC_2BPPV1_IMG}else return null;if(n===Bs||n===zs||n===Gs||n===Hs||n===Vs||n===Rr||n===ks)if(s=t.get("WEBGL_compressed_texture_etc"),s!==null){if(n===Bs||n===zs)return a===$t?s.COMPRESSED_SRGB8_ETC2:s.COMPRESSED_RGB8_ETC2;if(n===Gs)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ETC2_EAC:s.COMPRESSED_RGBA8_ETC2_EAC;if(n===Hs)return s.COMPRESSED_R11_EAC;if(n===Vs)return s.COMPRESSED_SIGNED_R11_EAC;if(n===Rr)return s.COMPRESSED_RG11_EAC;if(n===ks)return s.COMPRESSED_SIGNED_RG11_EAC}else return null;if(n===Ws||n===Xs||n===Ys||n===qs||n===Zs||n===Ks||n===$s||n===Js||n===Qs||n===js||n===ta||n===ea||n===na||n===ia)if(s=t.get("WEBGL_compressed_texture_astc"),s!==null){if(n===Ws)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_4x4_KHR:s.COMPRESSED_RGBA_ASTC_4x4_KHR;if(n===Xs)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_5x4_KHR:s.COMPRESSED_RGBA_ASTC_5x4_KHR;if(n===Ys)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_5x5_KHR:s.COMPRESSED_RGBA_ASTC_5x5_KHR;if(n===qs)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_6x5_KHR:s.COMPRESSED_RGBA_ASTC_6x5_KHR;if(n===Zs)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_6x6_KHR:s.COMPRESSED_RGBA_ASTC_6x6_KHR;if(n===Ks)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_8x5_KHR:s.COMPRESSED_RGBA_ASTC_8x5_KHR;if(n===$s)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_8x6_KHR:s.COMPRESSED_RGBA_ASTC_8x6_KHR;if(n===Js)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_8x8_KHR:s.COMPRESSED_RGBA_ASTC_8x8_KHR;if(n===Qs)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_10x5_KHR:s.COMPRESSED_RGBA_ASTC_10x5_KHR;if(n===js)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_10x6_KHR:s.COMPRESSED_RGBA_ASTC_10x6_KHR;if(n===ta)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_10x8_KHR:s.COMPRESSED_RGBA_ASTC_10x8_KHR;if(n===ea)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_10x10_KHR:s.COMPRESSED_RGBA_ASTC_10x10_KHR;if(n===na)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_12x10_KHR:s.COMPRESSED_RGBA_ASTC_12x10_KHR;if(n===ia)return a===$t?s.COMPRESSED_SRGB8_ALPHA8_ASTC_12x12_KHR:s.COMPRESSED_RGBA_ASTC_12x12_KHR}else return null;if(n===ra||n===sa||n===aa)if(s=t.get("EXT_texture_compression_bptc"),s!==null){if(n===ra)return a===$t?s.COMPRESSED_SRGB_ALPHA_BPTC_UNORM_EXT:s.COMPRESSED_RGBA_BPTC_UNORM_EXT;if(n===sa)return s.COMPRESSED_RGB_BPTC_SIGNED_FLOAT_EXT;if(n===aa)return s.COMPRESSED_RGB_BPTC_UNSIGNED_FLOAT_EXT}else return null;if(n===oa||n===la||n===Cr||n===ca)if(s=t.get("EXT_texture_compression_rgtc"),s!==null){if(n===oa)return s.COMPRESSED_RED_RGTC1_EXT;if(n===la)return s.COMPRESSED_SIGNED_RED_RGTC1_EXT;if(n===Cr)return s.COMPRESSED_RED_GREEN_RGTC2_EXT;if(n===ca)return s.COMPRESSED_SIGNED_RED_GREEN_RGTC2_EXT}else return null;return n===Bi?i.UNSIGNED_INT_24_8:i[n]!==void 0?i[n]:null}return{convert:e}}const Tm=`
void main() {

	gl_Position = vec4( position, 1.0 );

}`,Am=`
uniform sampler2DArray depthColor;
uniform float depthWidth;
uniform float depthHeight;

void main() {

	vec2 coord = vec2( gl_FragCoord.x / depthWidth, gl_FragCoord.y / depthHeight );

	if ( coord.x >= 1.0 ) {

		gl_FragDepth = texture( depthColor, vec3( coord.x - 1.0, coord.y, 1 ) ).r;

	} else {

		gl_FragDepth = texture( depthColor, vec3( coord.x, coord.y, 0 ) ).r;

	}

}`;class wm{constructor(){this.texture=null,this.mesh=null,this.depthNear=0,this.depthFar=0}init(t,e){if(this.texture===null){const n=new Pl(t.texture);(t.depthNear!==e.depthNear||t.depthFar!==e.depthFar)&&(this.depthNear=t.depthNear,this.depthFar=t.depthFar),this.texture=n}}getMesh(t){if(this.texture!==null&&this.mesh===null){const e=t.cameras[0].viewport,n=new hn({vertexShader:Tm,fragmentShader:Am,uniforms:{depthColor:{value:this.texture},depthWidth:{value:e.z},depthHeight:{value:e.w}}});this.mesh=new je(new zr(20,20),n)}return this.mesh}reset(){this.texture=null,this.mesh=null}getDepthTexture(){return this.texture}}class Rm extends Un{constructor(t,e){super();const n=this;let r=null,s=1,a=null,o="local-floor",c=1,l=null,d=null,p=null,h=null,_=null,v=null;const M=typeof XRWebGLBinding<"u",m=new wm,u={},A=e.getContextAttributes();let P=null,S=null;const b=[],y=[],w=new Ct;let x=null,T=null;const L=new qe;L.viewport=new ce;const F=new qe;F.viewport=new ce;const G=[L,F],k=new Lh;let U=null,H=null;this.cameraAutoUpdate=!0,this.enabled=!1,this.isPresenting=!1,this.getController=function(Y){let Q=b[Y];return Q===void 0&&(Q=new jr,b[Y]=Q),Q.getTargetRaySpace()},this.getControllerGrip=function(Y){let Q=b[Y];return Q===void 0&&(Q=new jr,b[Y]=Q),Q.getGripSpace()},this.getHand=function(Y){let Q=b[Y];return Q===void 0&&(Q=new jr,b[Y]=Q),Q.getHandSpace()};function K(Y){const Q=y.indexOf(Y.inputSource);if(Q===-1)return;const gt=b[Q];gt!==void 0&&(gt.update(Y.inputSource,Y.frame,l||a),gt.dispatchEvent({type:Y.type,data:Y.inputSource}))}function Z(){r.removeEventListener("select",K),r.removeEventListener("selectstart",K),r.removeEventListener("selectend",K),r.removeEventListener("squeeze",K),r.removeEventListener("squeezestart",K),r.removeEventListener("squeezeend",K),r.removeEventListener("end",Z),r.removeEventListener("inputsourceschange",nt);for(let Y=0;Y<b.length;Y++){const Q=y[Y];Q!==null&&(y[Y]=null,b[Y].disconnect(Q))}U=null,H=null,m.reset();for(const Y in u)delete u[Y];if(t.setRenderTarget(P),_=null,h=null,p=null,r=null,S=null,Yt.stop(),n.isPresenting=!1,t.setPixelRatio(x),t.setSize(w.width,w.height,!1),T!==null){const Y=T.camera;Y.fov=T.fov,Y.zoom=T.zoom,Y.updateProjectionMatrix(),T=null}n.dispatchEvent({type:"sessionend"})}this.setFramebufferScaleFactor=function(Y){s=Y,n.isPresenting===!0&&Rt("WebXRManager: Cannot change framebuffer scale while presenting.")},this.setReferenceSpaceType=function(Y){o=Y,n.isPresenting===!0&&Rt("WebXRManager: Cannot change reference space type while presenting.")},this.getReferenceSpace=function(){return l||a},this.setReferenceSpace=function(Y){l=Y},this.getBaseLayer=function(){return h!==null?h:_},this.getBinding=function(){return p===null&&M&&(p=new XRWebGLBinding(r,e)),p},this.getFrame=function(){return v},this.getSession=function(){return r},this.setSession=async function(Y){if(r=Y,r!==null){if(P=t.getRenderTarget(),r.addEventListener("select",K),r.addEventListener("selectstart",K),r.addEventListener("selectend",K),r.addEventListener("squeeze",K),r.addEventListener("squeezestart",K),r.addEventListener("squeezeend",K),r.addEventListener("end",Z),r.addEventListener("inputsourceschange",nt),A.xrCompatible!==!0&&await e.makeXRCompatible(),x=t.getPixelRatio(),t.getSize(w),M&&"createProjectionLayer"in XRWebGLBinding.prototype){let gt=null,Pt=null,mt=null;A.depth&&(mt=A.stencil?e.DEPTH24_STENCIL8:e.DEPTH_COMPONENT24,gt=A.stencil?Vn:En,Pt=A.stencil?Bi:ln);const Ft={colorFormat:e.RGBA8,depthFormat:mt,scaleFactor:s};p=this.getBinding(),h=p.createProjectionLayer(Ft),r.updateRenderState({layers:[h]}),t.setPixelRatio(1),t.setSize(h.textureWidth,h.textureHeight,!1),S=new Je(h.textureWidth,h.textureHeight,{format:$e,type:Be,depthTexture:new Hi(h.textureWidth,h.textureHeight,Pt,void 0,void 0,void 0,void 0,void 0,void 0,gt),stencilBuffer:A.stencil,colorSpace:t.outputColorSpace,samples:A.antialias?4:0,resolveDepthBuffer:h.ignoreDepthValues===!1,resolveStencilBuffer:h.ignoreDepthValues===!1,storeMultisampledDepthBuffer:h.ignoreDepthValues===!1,storeMultisampledStencilBuffer:h.ignoreDepthValues===!1})}else{const gt={antialias:A.antialias,alpha:!0,depth:A.depth,stencil:A.stencil,framebufferScaleFactor:s};_=new XRWebGLLayer(r,e,gt),r.updateRenderState({baseLayer:_}),t.setPixelRatio(1),t.setSize(_.framebufferWidth,_.framebufferHeight,!1),S=new Je(_.framebufferWidth,_.framebufferHeight,{format:$e,type:Be,colorSpace:t.outputColorSpace,stencilBuffer:A.stencil,resolveDepthBuffer:_.ignoreDepthValues===!1,resolveStencilBuffer:_.ignoreDepthValues===!1,storeMultisampledDepthBuffer:_.ignoreDepthValues===!1,storeMultisampledStencilBuffer:_.ignoreDepthValues===!1})}S.isXRRenderTarget=!0,this.setFoveation(c),l=null,a=await r.requestReferenceSpace(o),Yt.setContext(r),Yt.start(),n.isPresenting=!0,n.dispatchEvent({type:"sessionstart"})}},this.getEnvironmentBlendMode=function(){if(r!==null)return r.environmentBlendMode},this.getDepthTexture=function(){return m.getDepthTexture()};function nt(Y){for(let Q=0;Q<Y.removed.length;Q++){const gt=Y.removed[Q],Pt=y.indexOf(gt);Pt>=0&&(y[Pt]=null,b[Pt].disconnect(gt))}for(let Q=0;Q<Y.added.length;Q++){const gt=Y.added[Q];let Pt=y.indexOf(gt);if(Pt===-1){for(let Ft=0;Ft<b.length;Ft++)if(Ft>=y.length){y.push(gt),Pt=Ft;break}else if(y[Ft]===null){y[Ft]=gt,Pt=Ft;break}if(Pt===-1)break}const mt=b[Pt];mt&&mt.connect(gt)}}const X=new I,j=new I;function et(Y,Q,gt){X.setFromMatrixPosition(Q.matrixWorld),j.setFromMatrixPosition(gt.matrixWorld);const Pt=X.distanceTo(j),mt=Q.projectionMatrix.elements,Ft=gt.projectionMatrix.elements,pe=mt[14]/(mt[10]-1),Ot=mt[14]/(mt[10]+1),Xt=(mt[9]+1)/mt[5],ie=(mt[9]-1)/mt[5],zt=(mt[8]-1)/mt[0],oe=(Ft[8]+1)/Ft[0],xe=pe*zt,Le=pe*oe,le=Pt/(-zt+oe),ue=le*-zt;if(Q.matrixWorld.decompose(Y.position,Y.quaternion,Y.scale),Y.translateX(ue),Y.translateZ(le),Y.matrixWorld.compose(Y.position,Y.quaternion,Y.scale),Y.matrixWorldInverse.copy(Y.matrixWorld).invert(),mt[10]===-1)Y.projectionMatrix.copy(Q.projectionMatrix),Y.projectionMatrixInverse.copy(Q.projectionMatrixInverse);else{const D=pe+le,Ee=Ot+le,Kt=xe-ue,E=Le+(Pt-ue),f=Xt*Ot/Ee*D,N=ie*Ot/Ee*D;Y.projectionMatrix.makePerspective(Kt,E,f,N,D,Ee),Y.projectionMatrixInverse.copy(Y.projectionMatrix).invert()}}function At(Y,Q){Q===null?Y.matrixWorld.copy(Y.matrix):Y.matrixWorld.multiplyMatrices(Q.matrixWorld,Y.matrix),Y.matrixWorldInverse.copy(Y.matrixWorld).invert()}this.updateCamera=function(Y){if(r===null)return;let Q=Y.near,gt=Y.far;m.texture!==null&&(m.depthNear>0&&(Q=m.depthNear),m.depthFar>0&&(gt=m.depthFar)),k.near=F.near=L.near=Q,k.far=F.far=L.far=gt,(U!==k.near||H!==k.far)&&(r.updateRenderState({depthNear:k.near,depthFar:k.far}),U=k.near,H=k.far),k.layers.mask=Y.layers.mask|6,L.layers.mask=k.layers.mask&-5,F.layers.mask=k.layers.mask&-3;const Pt=Y.parent,mt=k.cameras;At(k,Pt);for(let Ft=0;Ft<mt.length;Ft++)At(mt[Ft],Pt);mt.length===2?et(k,L,F):k.projectionMatrix.copy(L.projectionMatrix),T===null&&Y.isPerspectiveCamera&&(T={camera:Y,fov:Y.fov,zoom:Y.zoom}),bt(Y,k,Pt)};function bt(Y,Q,gt){gt===null?Y.matrix.copy(Q.matrixWorld):(Y.matrix.copy(gt.matrixWorld),Y.matrix.invert(),Y.matrix.multiply(Q.matrixWorld)),Y.matrix.decompose(Y.position,Y.quaternion,Y.scale),Y.updateMatrixWorld(!0),Y.projectionMatrix.copy(Q.projectionMatrix),Y.projectionMatrixInverse.copy(Q.projectionMatrixInverse),Y.isPerspectiveCamera&&(Y.fov=Gi*2*Math.atan(1/Y.projectionMatrix.elements[5]),Y.zoom=1)}this.getCamera=function(){return k},this.getFoveation=function(){if(!(h===null&&_===null))return c},this.setFoveation=function(Y){c=Y,h!==null&&(h.fixedFoveation=Y),_!==null&&_.fixedFoveation!==void 0&&(_.fixedFoveation=Y)},this.hasDepthSensing=function(){return m.texture!==null},this.getDepthSensingMesh=function(){return m.getMesh(k)},this.getCameraTexture=function(Y){return u[Y]};let ne=null;function Vt(Y,Q){if(d=Q.getViewerPose(l||a),v=Q,d!==null){const gt=d.views;_!==null&&(t.setRenderTargetFramebuffer(S,_.framebuffer),t.setRenderTarget(S));let Pt=!1;gt.length!==k.cameras.length&&(k.cameras.length=0,Pt=!0);for(let Ot=0;Ot<gt.length;Ot++){const Xt=gt[Ot];let ie=null;if(_!==null)ie=_.getViewport(Xt);else{const oe=p.getViewSubImage(h,Xt);ie=oe.viewport,Ot===0&&(t.setRenderTargetTextures(S,oe.colorTexture,oe.depthStencilTexture),t.setRenderTarget(S))}let zt=G[Ot];zt===void 0&&(zt=new qe,zt.layers.enable(Ot),zt.viewport=new ce,G[Ot]=zt),zt.matrix.fromArray(Xt.transform.matrix),zt.matrix.decompose(zt.position,zt.quaternion,zt.scale),zt.projectionMatrix.fromArray(Xt.projectionMatrix),zt.projectionMatrixInverse.copy(zt.projectionMatrix).invert(),zt.viewport.set(ie.x,ie.y,ie.width,ie.height),Ot===0&&(k.matrix.copy(zt.matrix),k.matrix.decompose(k.position,k.quaternion,k.scale)),Pt===!0&&k.cameras.push(zt)}const mt=r.enabledFeatures;if(mt&&mt.includes("depth-sensing")&&r.depthUsage=="gpu-optimized"&&M){p=n.getBinding();const Ot=p.getDepthInformation(gt[0]);Ot&&Ot.isValid&&Ot.texture&&m.init(Ot,r.renderState)}if(mt&&mt.includes("camera-access")&&M){t.state.unbindTexture(),p=n.getBinding();for(let Ot=0;Ot<gt.length;Ot++){const Xt=gt[Ot].camera;if(Xt){let ie=u[Xt];ie||(ie=new Pl,u[Xt]=ie);const zt=p.getCameraImage(Xt);ie.sourceTexture=zt}}}}for(let gt=0;gt<b.length;gt++){const Pt=y[gt],mt=b[gt];Pt!==null&&mt!==void 0&&mt.update(Pt,Q,l||a)}ne&&ne(Y,Q),Q.detectedPlanes&&n.dispatchEvent({type:"planesdetected",data:Q}),v=null}const Yt=new Ul;Yt.setAnimationLoop(Vt),this.setAnimationLoop=function(Y){ne=Y},this.dispose=function(){}}}const Cm=new ee,Hl=new Dt;Hl.set(-1,0,0,0,1,0,0,0,1);function Pm(i,t){function e(m,u){m.matrixAutoUpdate===!0&&m.updateMatrix(),u.value.copy(m.matrix)}function n(m,u){u.color.getRGB(m.fogColor.value,Ll(i)),u.isFog?(m.fogNear.value=u.near,m.fogFar.value=u.far):u.isFogExp2&&(m.fogDensity.value=u.density)}function r(m,u,A,P,S){u.isNodeMaterial?u.uniformsNeedUpdate=!1:u.isMeshBasicMaterial?s(m,u):u.isMeshLambertMaterial?(s(m,u),u.envMap&&(m.envMapIntensity.value=u.envMapIntensity)):u.isMeshToonMaterial?(s(m,u),p(m,u)):u.isMeshPhongMaterial?(s(m,u),d(m,u),u.envMap&&(m.envMapIntensity.value=u.envMapIntensity)):u.isMeshStandardMaterial?(s(m,u),h(m,u),u.isMeshPhysicalMaterial&&_(m,u,S)):u.isMeshMatcapMaterial?(s(m,u),v(m,u)):u.isMeshDepthMaterial?s(m,u):u.isMeshDistanceMaterial?(s(m,u),M(m,u)):u.isMeshNormalMaterial?s(m,u):u.isLineBasicMaterial?(a(m,u),u.isLineDashedMaterial&&o(m,u)):u.isPointsMaterial?c(m,u,A,P):u.isSpriteMaterial?l(m,u):u.isShadowMaterial?(m.color.value.copy(u.color),m.opacity.value=u.opacity):u.isShaderMaterial&&(u.uniformsNeedUpdate=!1)}function s(m,u){m.opacity.value=u.opacity,u.color&&m.diffuse.value.copy(u.color),u.emissive&&m.emissive.value.copy(u.emissive).multiplyScalar(u.emissiveIntensity),u.map&&(m.map.value=u.map,e(u.map,m.mapTransform)),u.alphaMap&&(m.alphaMap.value=u.alphaMap,e(u.alphaMap,m.alphaMapTransform)),u.bumpMap&&(m.bumpMap.value=u.bumpMap,e(u.bumpMap,m.bumpMapTransform),m.bumpScale.value=u.bumpScale,u.side===Ue&&(m.bumpScale.value*=-1)),u.normalMap&&(m.normalMap.value=u.normalMap,e(u.normalMap,m.normalMapTransform),m.normalScale.value.copy(u.normalScale),u.side===Ue&&m.normalScale.value.negate()),u.displacementMap&&(m.displacementMap.value=u.displacementMap,e(u.displacementMap,m.displacementMapTransform),m.displacementScale.value=u.displacementScale,m.displacementBias.value=u.displacementBias),u.emissiveMap&&(m.emissiveMap.value=u.emissiveMap,e(u.emissiveMap,m.emissiveMapTransform)),u.specularMap&&(m.specularMap.value=u.specularMap,e(u.specularMap,m.specularMapTransform)),u.alphaTest>0&&(m.alphaTest.value=u.alphaTest);const A=t.get(u),P=A.envMap,S=A.envMapRotation;P&&(m.envMap.value=P,m.envMapRotation.value.setFromMatrix4(Cm.makeRotationFromEuler(S)).transpose(),P.isCubeTexture&&P.isRenderTargetTexture===!1&&m.envMapRotation.value.premultiply(Hl),m.reflectivity.value=u.reflectivity,m.ior.value=u.ior,m.refractionRatio.value=u.refractionRatio),u.lightMap&&(m.lightMap.value=u.lightMap,m.lightMapIntensity.value=u.lightMapIntensity,e(u.lightMap,m.lightMapTransform)),u.aoMap&&(m.aoMap.value=u.aoMap,m.aoMapIntensity.value=u.aoMapIntensity,e(u.aoMap,m.aoMapTransform))}function a(m,u){m.diffuse.value.copy(u.color),m.opacity.value=u.opacity,u.map&&(m.map.value=u.map,e(u.map,m.mapTransform))}function o(m,u){m.dashSize.value=u.dashSize,m.totalSize.value=u.dashSize+u.gapSize,m.scale.value=u.scale}function c(m,u,A,P){m.diffuse.value.copy(u.color),m.opacity.value=u.opacity,m.size.value=u.size*A,m.scale.value=P*.5,u.map&&(m.map.value=u.map,e(u.map,m.uvTransform)),u.alphaMap&&(m.alphaMap.value=u.alphaMap,e(u.alphaMap,m.alphaMapTransform)),u.alphaTest>0&&(m.alphaTest.value=u.alphaTest)}function l(m,u){m.diffuse.value.copy(u.color),m.opacity.value=u.opacity,m.rotation.value=u.rotation,u.map&&(m.map.value=u.map,e(u.map,m.mapTransform)),u.alphaMap&&(m.alphaMap.value=u.alphaMap,e(u.alphaMap,m.alphaMapTransform)),u.alphaTest>0&&(m.alphaTest.value=u.alphaTest)}function d(m,u){m.specular.value.copy(u.specular),m.shininess.value=Math.max(u.shininess,1e-4)}function p(m,u){u.gradientMap&&(m.gradientMap.value=u.gradientMap)}function h(m,u){m.metalness.value=u.metalness,u.metalnessMap&&(m.metalnessMap.value=u.metalnessMap,e(u.metalnessMap,m.metalnessMapTransform)),m.roughness.value=u.roughness,u.roughnessMap&&(m.roughnessMap.value=u.roughnessMap,e(u.roughnessMap,m.roughnessMapTransform)),u.envMap&&(m.envMapIntensity.value=u.envMapIntensity)}function _(m,u,A){m.ior.value=u.ior,u.sheen>0&&(m.sheenColor.value.copy(u.sheenColor).multiplyScalar(u.sheen),m.sheenRoughness.value=u.sheenRoughness,u.sheenColorMap&&(m.sheenColorMap.value=u.sheenColorMap,e(u.sheenColorMap,m.sheenColorMapTransform)),u.sheenRoughnessMap&&(m.sheenRoughnessMap.value=u.sheenRoughnessMap,e(u.sheenRoughnessMap,m.sheenRoughnessMapTransform))),u.clearcoat>0&&(m.clearcoat.value=u.clearcoat,m.clearcoatRoughness.value=u.clearcoatRoughness,u.clearcoatMap&&(m.clearcoatMap.value=u.clearcoatMap,e(u.clearcoatMap,m.clearcoatMapTransform)),u.clearcoatRoughnessMap&&(m.clearcoatRoughnessMap.value=u.clearcoatRoughnessMap,e(u.clearcoatRoughnessMap,m.clearcoatRoughnessMapTransform)),u.clearcoatNormalMap&&(m.clearcoatNormalMap.value=u.clearcoatNormalMap,e(u.clearcoatNormalMap,m.clearcoatNormalMapTransform),m.clearcoatNormalScale.value.copy(u.clearcoatNormalScale),u.side===Ue&&m.clearcoatNormalScale.value.negate())),u.dispersion>0&&(m.dispersion.value=u.dispersion),u.retroreflectivity>0&&(m.retroreflectivity.value=u.retroreflectivity),u.iridescence>0&&(m.iridescence.value=u.iridescence,m.iridescenceIOR.value=u.iridescenceIOR,m.iridescenceThicknessMinimum.value=u.iridescenceThicknessRange[0],m.iridescenceThicknessMaximum.value=u.iridescenceThicknessRange[1],u.iridescenceMap&&(m.iridescenceMap.value=u.iridescenceMap,e(u.iridescenceMap,m.iridescenceMapTransform)),u.iridescenceThicknessMap&&(m.iridescenceThicknessMap.value=u.iridescenceThicknessMap,e(u.iridescenceThicknessMap,m.iridescenceThicknessMapTransform))),u.transmission>0&&(m.transmission.value=u.transmission,m.transmissionSamplerMap.value=A.texture,m.transmissionSamplerSize.value.set(A.width,A.height),u.transmissionMap&&(m.transmissionMap.value=u.transmissionMap,e(u.transmissionMap,m.transmissionMapTransform)),m.thickness.value=u.thickness,u.thicknessMap&&(m.thicknessMap.value=u.thicknessMap,e(u.thicknessMap,m.thicknessMapTransform)),m.attenuationDistance.value=u.attenuationDistance,m.attenuationColor.value.copy(u.attenuationColor)),u.anisotropy>0&&(m.anisotropyVector.value.set(u.anisotropy*Math.cos(u.anisotropyRotation),u.anisotropy*Math.sin(u.anisotropyRotation)),u.anisotropyMap&&(m.anisotropyMap.value=u.anisotropyMap,e(u.anisotropyMap,m.anisotropyMapTransform))),m.specularIntensity.value=u.specularIntensity,m.specularColor.value.copy(u.specularColor),u.specularColorMap&&(m.specularColorMap.value=u.specularColorMap,e(u.specularColorMap,m.specularColorMapTransform)),u.specularIntensityMap&&(m.specularIntensityMap.value=u.specularIntensityMap,e(u.specularIntensityMap,m.specularIntensityMapTransform))}function v(m,u){u.matcap&&(m.matcap.value=u.matcap)}function M(m,u){const A=t.get(u).light;m.referencePosition.value.setFromMatrixPosition(A.matrixWorld),m.nearDistance.value=A.shadow.camera.near,m.farDistance.value=A.shadow.camera.far}return{refreshFogUniforms:n,refreshMaterialUniforms:r}}function Dm(i,t,e,n){let r={},s={},a=[];const o=i.getParameter(i.MAX_UNIFORM_BUFFER_BINDINGS);function c(S,b){const y=b.program;n.uniformBlockBinding(S,y)}function l(S,b){let y=r[S.id];y===void 0&&(m(S),y=d(S),r[S.id]=y,S.addEventListener("dispose",A));const w=b.program;n.updateUBOMapping(S,w);const x=t.render.frame;s[S.id]!==x&&(h(S),s[S.id]=x)}function d(S){const b=p();S.__bindingPointIndex=b;const y=i.createBuffer(),w=S.__size,x=S.usage;return i.bindBuffer(i.UNIFORM_BUFFER,y),i.bufferData(i.UNIFORM_BUFFER,w,x),i.bindBuffer(i.UNIFORM_BUFFER,null),i.bindBufferBase(i.UNIFORM_BUFFER,b,y),y}function p(){for(let S=0;S<o;S++)if(a.indexOf(S)===-1)return a.push(S),S;return Wt("WebGLRenderer: Maximum number of simultaneously usable uniforms groups reached."),0}function h(S){const b=r[S.id],y=S.uniforms,w=S.__cache;i.bindBuffer(i.UNIFORM_BUFFER,b);for(let x=0,T=y.length;x<T;x++){const L=y[x];if(Array.isArray(L))for(let F=0,G=L.length;F<G;F++)_(L[F],x,F,w);else _(L,x,0,w)}i.bindBuffer(i.UNIFORM_BUFFER,null)}function _(S,b,y,w){if(M(S,b,y,w)===!0){const x=S.__offset,T=S.value;if(Array.isArray(T)){let L=0;for(let F=0;F<T.length;F++){const G=T[F],k=u(G);v(G,S.__data,L),typeof G!="number"&&typeof G!="boolean"&&!G.isMatrix3&&!ArrayBuffer.isView(G)&&(L+=k.storage/Float32Array.BYTES_PER_ELEMENT)}}else v(T,S.__data,0);i.bufferSubData(i.UNIFORM_BUFFER,x,S.__data)}}function v(S,b,y){typeof S=="number"||typeof S=="boolean"?b[0]=S:S.isMatrix3?(b[0]=S.elements[0],b[1]=S.elements[1],b[2]=S.elements[2],b[3]=0,b[4]=S.elements[3],b[5]=S.elements[4],b[6]=S.elements[5],b[7]=0,b[8]=S.elements[6],b[9]=S.elements[7],b[10]=S.elements[8],b[11]=0):ArrayBuffer.isView(S)?b.set(new S.constructor(S.buffer,S.byteOffset,b.length)):S.toArray(b,y)}function M(S,b,y,w){const x=S.value,T=b+"_"+y;if(w[T]===void 0)return typeof x=="number"||typeof x=="boolean"?w[T]=x:ArrayBuffer.isView(x)?w[T]=x.slice():w[T]=x.clone(),!0;{const L=w[T];if(typeof x=="number"||typeof x=="boolean"){if(L!==x)return w[T]=x,!0}else{if(ArrayBuffer.isView(x))return!0;if(L.equals(x)===!1)return L.copy(x),!0}}return!1}function m(S){const b=S.uniforms;let y=0;const w=16;for(let T=0,L=b.length;T<L;T++){const F=Array.isArray(b[T])?b[T]:[b[T]];for(let G=0,k=F.length;G<k;G++){const U=F[G],H=Array.isArray(U.value)?U.value:[U.value];for(let K=0,Z=H.length;K<Z;K++){const nt=H[K],X=u(nt),j=y%w,et=j%X.boundary,At=j+et;y+=et,At!==0&&w-At<X.storage&&(y+=w-At),U.__data=new Float32Array(X.storage/Float32Array.BYTES_PER_ELEMENT),U.__offset=y,y+=X.storage}}}const x=y%w;return x>0&&(y+=w-x),S.__size=y,S.__cache={},this}function u(S){const b={boundary:0,storage:0};return typeof S=="number"||typeof S=="boolean"?(b.boundary=4,b.storage=4):S.isVector2?(b.boundary=8,b.storage=8):S.isVector3||S.isColor?(b.boundary=16,b.storage=12):S.isVector4?(b.boundary=16,b.storage=16):S.isMatrix3?(b.boundary=48,b.storage=48):S.isMatrix4?(b.boundary=64,b.storage=64):S.isTexture?Rt("WebGLRenderer: Texture samplers can not be part of an uniforms group."):ArrayBuffer.isView(S)?(b.boundary=16,b.storage=S.byteLength):Rt("WebGLRenderer: Unsupported uniform value type.",S),b}function A(S){const b=S.target;b.removeEventListener("dispose",A);const y=a.indexOf(b.__bindingPointIndex);a.splice(y,1),i.deleteBuffer(r[b.id]),delete r[b.id],delete s[b.id]}function P(){for(const S in r)i.deleteBuffer(r[S]);a=[],r={},s={}}return{bind:c,update:l,dispose:P}}const Lm=new Uint16Array([12469,15057,12620,14925,13266,14620,13807,14376,14323,13990,14545,13625,14713,13328,14840,12882,14931,12528,14996,12233,15039,11829,15066,11525,15080,11295,15085,10976,15082,10705,15073,10495,13880,14564,13898,14542,13977,14430,14158,14124,14393,13732,14556,13410,14702,12996,14814,12596,14891,12291,14937,11834,14957,11489,14958,11194,14943,10803,14921,10506,14893,10278,14858,9960,14484,14039,14487,14025,14499,13941,14524,13740,14574,13468,14654,13106,14743,12678,14818,12344,14867,11893,14889,11509,14893,11180,14881,10751,14852,10428,14812,10128,14765,9754,14712,9466,14764,13480,14764,13475,14766,13440,14766,13347,14769,13070,14786,12713,14816,12387,14844,11957,14860,11549,14868,11215,14855,10751,14825,10403,14782,10044,14729,9651,14666,9352,14599,9029,14967,12835,14966,12831,14963,12804,14954,12723,14936,12564,14917,12347,14900,11958,14886,11569,14878,11247,14859,10765,14828,10401,14784,10011,14727,9600,14660,9289,14586,8893,14508,8533,15111,12234,15110,12234,15104,12216,15092,12156,15067,12010,15028,11776,14981,11500,14942,11205,14902,10752,14861,10393,14812,9991,14752,9570,14682,9252,14603,8808,14519,8445,14431,8145,15209,11449,15208,11451,15202,11451,15190,11438,15163,11384,15117,11274,15055,10979,14994,10648,14932,10343,14871,9936,14803,9532,14729,9218,14645,8742,14556,8381,14461,8020,14365,7603,15273,10603,15272,10607,15267,10619,15256,10631,15231,10614,15182,10535,15118,10389,15042,10167,14963,9787,14883,9447,14800,9115,14710,8665,14615,8318,14514,7911,14411,7507,14279,7198,15314,9675,15313,9683,15309,9712,15298,9759,15277,9797,15229,9773,15166,9668,15084,9487,14995,9274,14898,8910,14800,8539,14697,8234,14590,7790,14479,7409,14367,7067,14178,6621,15337,8619,15337,8631,15333,8677,15325,8769,15305,8871,15264,8940,15202,8909,15119,8775,15022,8565,14916,8328,14804,8009,14688,7614,14569,7287,14448,6888,14321,6483,14088,6171,15350,7402,15350,7419,15347,7480,15340,7613,15322,7804,15287,7973,15229,8057,15148,8012,15046,7846,14933,7611,14810,7357,14682,7069,14552,6656,14421,6316,14251,5948,14007,5528,15356,5942,15356,5977,15353,6119,15348,6294,15332,6551,15302,6824,15249,7044,15171,7122,15070,7050,14949,6861,14818,6611,14679,6349,14538,6067,14398,5651,14189,5311,13935,4958,15359,4123,15359,4153,15356,4296,15353,4646,15338,5160,15311,5508,15263,5829,15188,6042,15088,6094,14966,6001,14826,5796,14678,5543,14527,5287,14377,4985,14133,4586,13869,4257,15360,1563,15360,1642,15358,2076,15354,2636,15341,3350,15317,4019,15273,4429,15203,4732,15105,4911,14981,4932,14836,4818,14679,4621,14517,4386,14359,4156,14083,3795,13808,3437,15360,122,15360,137,15358,285,15355,636,15344,1274,15322,2177,15281,2765,15215,3223,15120,3451,14995,3569,14846,3567,14681,3466,14511,3305,14344,3121,14037,2800,13753,2467,15360,0,15360,1,15359,21,15355,89,15346,253,15325,479,15287,796,15225,1148,15133,1492,15008,1749,14856,1882,14685,1886,14506,1783,14324,1608,13996,1398,13702,1183]);let rn=null;function Im(){return rn===null&&(rn=new Rl(Lm,16,16,Xn,cn),rn.name="DFG_LUT",rn.minFilter=Ae,rn.magFilter=Ae,rn.wrapS=xn,rn.wrapT=xn,rn.generateMipmaps=!1,rn.needsUpdate=!0),rn}class o_{constructor(t={}){const{canvas:e=Pc(),context:n=null,depth:r=!0,stencil:s=!1,alpha:a=!1,antialias:o=!1,premultipliedAlpha:c=!0,preserveDrawingBuffer:l=!1,powerPreference:d="default",failIfMajorPerformanceCaveat:p=!1,reversedDepthBuffer:h=!1,outputBufferType:_=Be}=t;this.isWebGLRenderer=!0;let v;if(n!==null){if(typeof WebGLRenderingContext<"u"&&n instanceof WebGLRenderingContext)throw new Error("THREE.WebGLRenderer: WebGL 1 is not supported since r163.");v=n.getContextAttributes().alpha}else v=a;const M=_,m=new Set([Sa,va,xa]),u=new Set([Be,ln,Oi,Bi,ma,_a]),A=new Uint32Array(4),P=new Int32Array(4),S=new I;let b=null,y=null;const w=[],x=[];let T=null;this.domElement=e,this.debug={checkShaderErrors:!0,diagnostics:{keywords:!1},onShaderError:null},this.autoClear=!0,this.autoClearColor=!0,this.autoClearDepth=!0,this.autoClearStencil=!0,this.sortObjects=!0,this.clippingPlanes=[],this.localClippingEnabled=!1,this.toneMapping=on,this.toneMappingExposure=1,this.transmissionResolutionScale=1;const L=this;let F=!1,G=null,k=null,U=null,H=null;this._outputColorSpace=He;let K=0,Z=0,nt=null,X=-1,j=null;const et=new ce,At=new ce;let bt=null;const ne=new Gt(0);let Vt=0,Yt=e.width,Y=e.height,Q=1,gt=null,Pt=null;const mt=new ce(0,0,Yt,Y),Ft=new ce(0,0,Yt,Y);let pe=!1;const Ot=new Aa;let Xt=!1,ie=!1;const zt=new ee,oe=new I,xe=new ce,Le={background:null,fog:null,environment:null,overrideMaterial:null,isScene:!0};let le=!1;function ue(){return nt===null?Q:1}let D=n;function Ee(g,R){return e.getContext(g,R)}let Kt,E,f,N,z,W,it,rt,q,J,st,Et,ct,at,yt,wt,It,C,ot,$,lt,ft,tt;try{const g={alpha:!0,depth:r,stencil:s,antialias:o,premultipliedAlpha:c,preserveDrawingBuffer:l,powerPreference:d,failIfMajorPerformanceCaveat:p};if("setAttribute"in e&&e.setAttribute("data-engine","three.js r186"),e.addEventListener("webglcontextlost",re,!1),e.addEventListener("webglcontextrestored",qt,!1),e.addEventListener("webglcontextcreationerror",ke,!1),D===null){const R="webgl2";if(D=Ee(R,g),D===null)throw Ee(R)?new Error("THREE.WebGLRenderer: Error creating WebGL context with your selected attributes."):new Error("THREE.WebGLRenderer: Error creating WebGL context.")}Tt()}catch(g){throw e.removeEventListener("webglcontextlost",re,!1),e.removeEventListener("webglcontextrestored",qt,!1),e.removeEventListener("webglcontextcreationerror",ke,!1),Wt("WebGLRenderer: "+g.message),g}function Tt(){Kt=new If(D),Kt.init(),lt=new bm(D,Kt),E=new yf(D,Kt,t,lt),f=new Em(D,Kt),E.reversedDepthBuffer&&h&&f.buffers.depth.setReversed(!0),k=D.createFramebuffer(),U=D.createFramebuffer(),H=D.createFramebuffer(),N=new Ff(D),z=new lm,W=new ym(D,Kt,f,z,E,lt,N),it=new Lf(L),rt=new Oh(D),ft=new Mf(D,rt),q=new Uf(D,rt,N,ft),J=new Bf(D,q,rt,ft,N),C=new Of(D,E,W),yt=new bf(z),st=new om(L,it,Kt,E,ft,yt),Et=new Pm(L,z),ct=new hm,at=new _m(Kt),It=new Sf(L,it,f,J,v,c),wt=new Mm(L,J,E),tt=new Dm(D,N,E,f),ot=new Ef(D,Kt,N),$=new Nf(D,Kt,N),N.programs=st.programs,L.capabilities=E,L.extensions=Kt,L.properties=z,L.renderLists=ct,L.shadowMap=wt,L.state=f,L.info=N}M!==Be&&(T=new Gf(M,e.width,e.height,o,r,s));const St=new Rm(L,D);this.xr=St,this.getContext=function(){return D},this.getContextAttributes=function(){return D.getContextAttributes()},this.forceContextLoss=function(){const g=Kt.get("WEBGL_lose_context");g&&g.loseContext()},this.forceContextRestore=function(){const g=Kt.get("WEBGL_lose_context");g&&g.restoreContext()},this.getPixelRatio=function(){return Q},this.setPixelRatio=function(g){g!==void 0&&(Q=g,this.setSize(Yt,Y,!1))},this.getSize=function(g){return g.set(Yt,Y)},this.setSize=function(g,R,V=!0){if(St.isPresenting){Rt("WebGLRenderer: Can't change size while VR device is presenting.");return}Yt=g,Y=R,e.width=Math.floor(g*Q),e.height=Math.floor(R*Q),V===!0&&(e.style.width=g+"px",e.style.height=R+"px"),T!==null&&T.setSize(e.width,e.height),this.setViewport(0,0,g,R)},this.getDrawingBufferSize=function(g){return g.set(Yt*Q,Y*Q).floor()},this.setDrawingBufferSize=function(g,R,V){Yt=g,Y=R,Q=V,e.width=Math.floor(g*V),e.height=Math.floor(R*V),this.setViewport(0,0,g,R)},this.setEffects=function(g){if(M===Be){Wt("WebGLRenderer: setEffects() requires outputBufferType set to HalfFloatType or FloatType.");return}if(g){for(let R=0;R<g.length;R++)if(g[R].isOutputPass===!0){Rt("WebGLRenderer: OutputPass is not needed in setEffects(). Tone mapping and color space conversion are applied automatically.");break}}T.setEffects(g||[])},this.getCurrentViewport=function(g){return g.copy(et)},this.getViewport=function(g){return g.copy(mt)},this.setViewport=function(g,R,V,O){g.isVector4?mt.set(g.x,g.y,g.z,g.w):mt.set(g,R,V,O),f.viewport(et.copy(mt).multiplyScalar(Q).round())},this.getScissor=function(g){return g.copy(Ft)},this.setScissor=function(g,R,V,O){g.isVector4?Ft.set(g.x,g.y,g.z,g.w):Ft.set(g,R,V,O),f.scissor(At.copy(Ft).multiplyScalar(Q).round())},this.getScissorTest=function(){return pe},this.setScissorTest=function(g){f.setScissorTest(pe=g)},this.setOpaqueSort=function(g){gt=g},this.setTransparentSort=function(g){Pt=g},this.getClearColor=function(g){return g.copy(It.getClearColor())},this.setClearColor=function(){It.setClearColor(...arguments)},this.getClearAlpha=function(){return It.getClearAlpha()},this.setClearAlpha=function(){It.setClearAlpha(...arguments)},this.clear=function(g=!0,R=!0,V=!0){let O=0;if(g){let B=!1;if(nt!==null){const dt=nt.texture.format;B=m.has(dt)}if(B){const dt=nt.texture.type,_t=u.has(dt),ut=It.getClearColor(),xt=It.getClearAlpha(),Mt=ut.r,Ut=ut.g,Bt=ut.b;_t?(A[0]=Mt,A[1]=Ut,A[2]=Bt,A[3]=xt,D.clearBufferuiv(D.COLOR,0,A)):(P[0]=Mt,P[1]=Ut,P[2]=Bt,P[3]=xt,D.clearBufferiv(D.COLOR,0,P))}else O|=D.COLOR_BUFFER_BIT}R&&(O|=D.DEPTH_BUFFER_BIT,this.state.buffers.depth.setMask(!0)),V&&(O|=D.STENCIL_BUFFER_BIT,this.state.buffers.stencil.setMask(4294967295)),O!==0&&D.clear(O)},this.clearColor=function(){this.clear(!0,!1,!1)},this.clearDepth=function(){this.clear(!1,!0,!1)},this.clearStencil=function(){this.clear(!1,!1,!0)},this.setNodesHandler=function(g){g.setRenderer(this),G=g},this.dispose=function(){e.removeEventListener("webglcontextlost",re,!1),e.removeEventListener("webglcontextrestored",qt,!1),e.removeEventListener("webglcontextcreationerror",ke,!1),It.dispose(),ct.dispose(),at.dispose(),z.dispose(),it.dispose(),J.dispose(),ft.dispose(),tt.dispose(),st.dispose(),St.dispose(),St.removeEventListener("sessionstart",Fa),St.removeEventListener("sessionend",Oa),Nn.stop()};function re(g){g.preventDefault(),Ir("WebGLRenderer: Context Lost."),F=!0}function qt(){Ir("WebGLRenderer: Context Restored."),F=!1;const g=N.autoReset,R=wt.enabled,V=wt.autoUpdate,O=wt.needsUpdate,B=wt.type;Tt(),N.autoReset=g,wt.enabled=R,wt.autoUpdate=V,wt.needsUpdate=O,wt.type=B}function ke(g){Wt("WebGLRenderer: A WebGL context could not be created. Reason: ",g.statusMessage)}function tn(g){const R=g.target;R.removeEventListener("dispose",tn),kl(R)}function kl(g){Wl(g),z.remove(g)}function Wl(g){const R=z.get(g).programs;R!==void 0&&(R.forEach(function(V){st.releaseProgram(V)}),g.isShaderMaterial&&st.releaseShaderCache(g))}this.renderBufferDirect=function(g,R,V,O,B,dt){R===null&&(R=Le);const _t=B.isMesh&&B.matrixWorld.determinantAffine()<0,ut=ql(g,R,V,O,B);f.setMaterial(O,_t);let xt=V.index,Mt=1;if(O.wireframe===!0){if(xt=q.getWireframeAttribute(V),xt===void 0)return;Mt=2}const Ut=V.drawRange,Bt=V.attributes.position;let vt=Ut.start*Mt,Zt=(Ut.start+Ut.count)*Mt;dt!==null&&(vt=Math.max(vt,dt.start*Mt),Zt=Math.min(Zt,(dt.start+dt.count)*Mt)),xt!==null?(vt=Math.max(vt,0),Zt=Math.min(Zt,xt.count)):Bt!=null&&(vt=Math.max(vt,0),Zt=Math.min(Zt,Bt.count));const de=Zt-vt;if(de<0||de===1/0)return;ft.setup(B,O,ut,V,xt);let ae,te=ot;if(xt!==null&&(ae=rt.get(xt),te=$,te.setIndex(ae)),B.isMesh)O.wireframe===!0?(f.setLineWidth(O.wireframeLinewidth*ue()),te.setMode(D.LINES)):te.setMode(D.TRIANGLES);else if(B.isLine){let ye=O.linewidth;ye===void 0&&(ye=1),f.setLineWidth(ye*ue()),B.isLineSegments?te.setMode(D.LINES):B.isLineLoop?te.setMode(D.LINE_LOOP):te.setMode(D.LINE_STRIP)}else B.isPoints?te.setMode(D.POINTS):B.isSprite&&te.setMode(D.TRIANGLES);if(B.isBatchedMesh)if(Kt.get("WEBGL_multi_draw"))te.renderMultiDraw(B._multiDrawStarts,B._multiDrawCounts,B._multiDrawCount);else{const ye=B._multiDrawStarts,pt=B._multiDrawCounts,Re=B._multiDrawCount,kt=xt?rt.get(xt).bytesPerElement:1,ze=z.get(O).currentProgram.getUniforms();for(let en=0;en<Re;en++)ze.setValue(D,"_gl_DrawID",en),te.render(ye[en]/kt,pt[en])}else if(B.isInstancedMesh)te.renderInstances(vt,de,B.count);else if(V.isInstancedBufferGeometry){const ye=V._maxInstanceCount!==void 0?V._maxInstanceCount:1/0,pt=Math.min(V.instanceCount,ye);te.renderInstances(vt,de,pt)}else te.render(vt,de)};function Na(g,R,V,O){G!==null&&g.isNodeMaterial&&G.setObject(O,g),Xt===!0&&yt.setState(g,V,!1),g.transparent===!0&&g.side===gn&&g.forceSinglePass===!1?(g.side=Ue,g.needsUpdate=!0,Wi(g,R,O),g.side=kn,g.needsUpdate=!0,Wi(g,R,O),g.side=gn):Wi(g,R,O)}this.compile=function(g,R,V=null){V===null&&(V=g),G!==null&&G.renderStart(g,R,V),y=at.get(V),y.init(R),x.push(y),V.traverseVisible(function(B){B.isLight&&B.layers.test(R.layers)&&(y.pushLight(B),B.castShadow&&y.pushShadow(B))}),g!==V&&g.traverseVisible(function(B){B.isLight&&B.layers.test(R.layers)&&(y.pushLight(B),B.castShadow&&y.pushShadow(B))}),y.setupLights(),G!==null&&G.updateLights(y.state.lightsArray),ie=this.localClippingEnabled,Xt=yt.init(this.clippingPlanes,ie),Xt===!0&&yt.setGlobalState(this.clippingPlanes,R),G!==null&&wt.render(y.state.shadowsArray,V,R);const O=new Set;return g.traverse(function(B){if(!(B.isMesh||B.isPoints||B.isLine||B.isSprite))return;const dt=B.material;if(dt)if(Array.isArray(dt))for(let _t=0;_t<dt.length;_t++){const ut=dt[_t];Na(ut,V,R,B),O.add(ut)}else Na(dt,V,R,B),O.add(dt)}),y=x.pop(),G!==null&&G.renderEnd(),O},this.compileAsync=function(g,R,V=null){const O=this.compile(g,R,V);return new Promise(B=>{function dt(){if(O.forEach(function(_t){const xt=z.get(_t).currentProgram;(xt===void 0||xt.isReady())&&O.delete(_t)}),O.size===0){B(g);return}setTimeout(dt,10)}Kt.get("KHR_parallel_shader_compile")!==null?dt():setTimeout(dt,10)})};let Vr=null;function Xl(g){Vr&&Vr(g)}function Fa(){Nn.stop()}function Oa(){Nn.start()}const Nn=new Ul;Nn.setAnimationLoop(Xl),typeof self<"u"&&Nn.setContext(self),this.setAnimationLoop=function(g){Vr=g,St.setAnimationLoop(g),g===null?Nn.stop():Nn.start()},St.addEventListener("sessionstart",Fa),St.addEventListener("sessionend",Oa),this.render=function(g,R){if(R!==void 0&&R.isCamera!==!0){Wt("WebGLRenderer.render: camera is not an instance of THREE.Camera.");return}if(F===!0)return;G!==null&&G.renderStart(g,R);const V=St.enabled===!0&&St.isPresenting===!0,O=T!==null&&(nt===null||V)&&T.begin(L,nt);if(g.matrixWorldAutoUpdate===!0&&g.updateMatrixWorld(),R.parent===null&&R.matrixWorldAutoUpdate===!0&&R.updateMatrixWorld(),St.enabled===!0&&St.isPresenting===!0&&(T===null||T.isCompositing()===!1)&&(St.cameraAutoUpdate===!0&&St.updateCamera(R),R=St.getCamera()),g.isScene===!0&&g.onBeforeRender(L,g,R,nt),y=at.get(g,x.length),y.init(R),y.state.textureUnits=W.getTextureUnits(),x.push(y),zt.multiplyMatrices(R.projectionMatrix,R.matrixWorldInverse),Ot.setFromProjectionMatrix(zt,an,R.reversedDepth),ie=this.localClippingEnabled,Xt=yt.init(this.clippingPlanes,ie),b=ct.get(g,w.length),b.init(),w.push(b),St.enabled===!0&&St.isPresenting===!0){const _t=L.xr.getDepthSensingMesh();_t!==null&&kr(_t,R,-1/0,L.sortObjects)}kr(g,R,0,L.sortObjects),b.finish(),G!==null&&G.updateLights(y.state.lightsArray),L.sortObjects===!0&&b.sort(gt,Pt),le=St.enabled===!1||St.isPresenting===!1||St.hasDepthSensing()===!1,le&&It.addToRenderList(b,g),this.info.render.frame++,this.info.autoReset===!0&&this.info.reset(),Xt===!0&&yt.beginShadows();const B=y.state.shadowsArray;if(wt.render(B,g,R),Xt===!0&&yt.endShadows(),(O&&T.hasRenderPass())===!1){const _t=b.opaque,ut=b.transmissive;if(y.setupLights(),R.isArrayCamera){const xt=R.cameras;if(ut.length>0)for(let Mt=0,Ut=xt.length;Mt<Ut;Mt++){const Bt=xt[Mt];za(_t,ut,g,Bt)}le&&It.render(g);for(let Mt=0,Ut=xt.length;Mt<Ut;Mt++){const Bt=xt[Mt];Ba(b,g,Bt,Bt.viewport)}}else ut.length>0&&za(_t,ut,g,R),le&&It.render(g),Ba(b,g,R)}nt!==null&&Z===0&&(W.updateMultisampleRenderTarget(nt),W.updateRenderTargetMipmap(nt)),O&&T.end(L),g.isScene===!0&&g.onAfterRender(L,g,R),ft.resetDefaultState(),X=-1,j=null,x.pop(),x.length>0?(y=x[x.length-1],W.setTextureUnits(y.state.textureUnits),Xt===!0&&yt.setGlobalState(L.clippingPlanes,y.state.camera)):y=null,w.pop(),w.length>0?b=w[w.length-1]:b=null,G!==null&&G.renderEnd()};function kr(g,R,V,O){if(g.visible===!1)return;if(g.layers.test(R.layers)){if(g.isGroup)V=g.renderOrder;else if(g.isLOD)g.autoUpdate===!0&&g.update(R);else if(g.isLightProbeGrid)y.pushLightProbeGrid(g);else if(g.isLight)y.pushLight(g),g.castShadow&&y.pushShadow(g);else if(g.isSprite){if(!g.frustumCulled||g.intersectsFrustum(Ot)){O&&xe.setFromMatrixPosition(g.matrixWorld).applyMatrix4(zt);const _t=J.update(g),ut=g.material;ut.visible&&b.push(g,_t,ut,V,xe.z,null,R)}}else if((g.isMesh||g.isLine||g.isPoints)&&(!g.frustumCulled||g.intersectsFrustum(Ot))){const _t=J.update(g),ut=g.material;if(O&&(g.boundingSphere!==void 0?(g.boundingSphere===null&&g.computeBoundingSphere(),xe.copy(g.boundingSphere.center)):(_t.boundingSphere===null&&_t.computeBoundingSphere(),xe.copy(_t.boundingSphere.center)),xe.applyMatrix4(g.matrixWorld).applyMatrix4(zt)),Array.isArray(ut)){const xt=_t.groups;for(let Mt=0,Ut=xt.length;Mt<Ut;Mt++){const Bt=xt[Mt],vt=ut[Bt.materialIndex];vt&&vt.visible&&b.push(g,_t,vt,V,xe.z,Bt,R)}}else ut.visible&&b.push(g,_t,ut,V,xe.z,null,R)}}const dt=g.children;for(let _t=0,ut=dt.length;_t<ut;_t++)kr(dt[_t],R,V,O)}function Ba(g,R,V,O){const{opaque:B,transmissive:dt,transparent:_t}=g;y.setupLightsView(V),Xt===!0&&yt.setGlobalState(L.clippingPlanes,V),O&&f.viewport(et.copy(O)),B.length>0&&ki(B,R,V),dt.length>0&&ki(dt,R,V),_t.length>0&&ki(_t,R,V),f.buffers.depth.setTest(!0),f.buffers.depth.setMask(!0),f.buffers.color.setMask(!0),f.setPolygonOffset(!1)}function za(g,R,V,O){if((V.isScene===!0?V.overrideMaterial:null)!==null)return;if(y.state.transmissionRenderTarget[O.id]===void 0){const vt=Kt.has("EXT_color_buffer_half_float")||Kt.has("EXT_color_buffer_float");y.state.transmissionRenderTarget[O.id]=new Je(1,1,{generateMipmaps:!0,type:vt?cn:Be,minFilter:Hn,samples:Math.max(4,E.samples),stencilBuffer:s,resolveDepthBuffer:!1,resolveStencilBuffer:!1,storeMultisampledDepthBuffer:!1,storeMultisampledStencilBuffer:!1,colorSpace:Ht.workingColorSpace})}const dt=y.state.transmissionRenderTarget[O.id],_t=O.viewport||et;dt.setSize(_t.z*L.transmissionResolutionScale,_t.w*L.transmissionResolutionScale);const ut=L.getRenderTarget(),xt=L.getActiveCubeFace(),Mt=L.getActiveMipmapLevel();L.setRenderTarget(dt),L.getClearColor(ne),Vt=L.getClearAlpha(),Vt<1&&L.setClearColor(16777215,.5),L.clear(),le&&It.render(V);const Ut=L.toneMapping;L.toneMapping=on;const Bt=O.viewport;if(O.viewport!==void 0&&(O.viewport=void 0),y.setupLightsView(O),Xt===!0&&yt.setGlobalState(L.clippingPlanes,O),ki(g,V,O),W.updateMultisampleRenderTarget(dt),W.updateRenderTargetMipmap(dt),Kt.has("WEBGL_multisampled_render_to_texture")===!1){let vt=!1;for(let Zt=0,de=R.length;Zt<de;Zt++){const ae=R[Zt],{object:te,geometry:ye,material:pt,group:Re}=ae;if(pt.side===gn&&te.layers.test(O.layers)){const kt=pt.side;pt.side=Ue,pt.needsUpdate=!0,Ga(te,V,O,ye,pt,Re),pt.side=kt,pt.needsUpdate=!0,vt=!0}}vt===!0&&(W.updateMultisampleRenderTarget(dt),W.updateRenderTargetMipmap(dt))}L.setRenderTarget(ut,xt,Mt),L.setClearColor(ne,Vt),Bt!==void 0&&(O.viewport=Bt),L.toneMapping=Ut}function ki(g,R,V){const O=R.isScene===!0?R.overrideMaterial:null;for(let B=0,dt=g.length;B<dt;B++){const _t=g[B],{object:ut,geometry:xt,group:Mt}=_t;let Ut=_t.material;Ut.allowOverride===!0&&O!==null&&(Ut=O),ut.layers.test(V.layers)&&Ga(ut,R,V,xt,Ut,Mt)}}function Ga(g,R,V,O,B,dt){G!==null&&B.isNodeMaterial&&G.setObject(g,B),g.onBeforeRender(L,R,V,O,B,dt),g.modelViewMatrix.multiplyMatrices(V.matrixWorldInverse,g.matrixWorld),g.normalMatrix.getNormalMatrix(g.modelViewMatrix),B.onBeforeRender(L,R,V,O,g,dt),B.transparent===!0&&B.side===gn&&B.forceSinglePass===!1?(B.side=Ue,B.needsUpdate=!0,L.renderBufferDirect(V,R,O,B,g,dt),B.side=kn,B.needsUpdate=!0,L.renderBufferDirect(V,R,O,B,g,dt),B.side=gn):L.renderBufferDirect(V,R,O,B,g,dt),g.onAfterRender(L,R,V,O,B,dt)}function Wi(g,R,V){R.isScene!==!0&&(R=Le);const O=z.get(g),B=y.state.lights,dt=y.state.shadowsArray,_t=B.state.version,ut=st.getParameters(g,B.state,dt,R,V,y.state.lightProbeGridArray),xt=st.getProgramCacheKey(ut);let Mt=O.programs;O.environment=g.isMeshStandardMaterial||g.isMeshLambertMaterial||g.isMeshPhongMaterial?R.environment:null,O.fog=R.fog;const Ut=g.isMeshStandardMaterial||g.isMeshLambertMaterial&&!g.envMap||g.isMeshPhongMaterial&&!g.envMap;O.envMap=it.get(g.envMap||O.environment,Ut),O.envMapRotation=O.environment!==null&&g.envMap===null?R.environmentRotation:g.envMapRotation,Mt===void 0&&(g.addEventListener("dispose",tn),Mt=new Map,O.programs=Mt);let Bt=Mt.get(xt);if(Bt!==void 0){if(O.currentProgram===Bt&&O.lightsStateVersion===_t)return Va(g,ut),Bt}else ut.uniforms=st.getUniforms(g),G!==null&&g.isNodeMaterial&&G.build(g,V,ut),g.onBeforeCompile(ut,L),Bt=st.acquireProgram(ut,xt),Mt.set(xt,Bt),O.uniforms=ut.uniforms;const vt=O.uniforms;return(!g.isShaderMaterial&&!g.isRawShaderMaterial||g.clipping===!0)&&(vt.clippingPlanes=yt.uniform),Va(g,ut),O.needsLights=Kl(g),O.lightsStateVersion=_t,O.needsLights&&(vt.ambientLightColor.value=B.state.ambient,vt.lightProbe.value=B.state.probe,vt.sunLights.value=B.state.sun,vt.sunLightShadows.value=B.state.sunShadow,vt.directionalLights.value=B.state.directional,vt.directionalLightShadows.value=B.state.directionalShadow,vt.spotLights.value=B.state.spot,vt.spotLightShadows.value=B.state.spotShadow,vt.rectAreaLights.value=B.state.rectArea,vt.ltc_1.value=B.state.rectAreaLTC1,vt.ltc_2.value=B.state.rectAreaLTC2,vt.pointLights.value=B.state.point,vt.pointLightShadows.value=B.state.pointShadow,vt.hemisphereLights.value=B.state.hemi,vt.sunShadowMatrix.value=B.state.sunShadowMatrix,vt.sunShadowCascade.value=B.state.sunShadowCascade,vt.directionalShadowMatrix.value=B.state.directionalShadowMatrix,vt.spotLightMatrix.value=B.state.spotLightMatrix,vt.spotLightMap.value=B.state.spotLightMap,vt.pointShadowMatrix.value=B.state.pointShadowMatrix),O.lightProbeGrid=y.state.lightProbeGridArray.length>0,O.currentProgram=Bt,O.uniformsList=null,Bt}function Ha(g){if(g.uniformsList===null){const R=g.currentProgram.getUniforms();g.uniformsList=wr.seqWithValue(R.seq,g.uniforms)}return g.uniformsList}function Va(g,R){const V=z.get(g);V.outputColorSpace=R.outputColorSpace,V.batching=R.batching,V.batchingColor=R.batchingColor,V.instancing=R.instancing,V.instancingColor=R.instancingColor,V.instancingMorph=R.instancingMorph,V.skinning=R.skinning,V.morphTargets=R.morphTargets,V.morphNormals=R.morphNormals,V.morphColors=R.morphColors,V.morphTargetsCount=R.morphTargetsCount,V.numClippingPlanes=R.numClippingPlanes,V.numIntersection=R.numClipIntersection,V.vertexAlphas=R.vertexAlphas,V.vertexTangents=R.vertexTangents,V.toneMapping=R.toneMapping}function Yl(g,R){if(g.length===0)return null;if(g.length===1)return g[0].texture!==null?g[0]:null;S.setFromMatrixPosition(R.matrixWorld);for(let V=0,O=g.length;V<O;V++){const B=g[V];if(B.texture!==null&&B.boundingBox.containsPoint(S))return B}return null}function ql(g,R,V,O,B){R.isScene!==!0&&(R=Le),W.resetTextureUnits();const dt=R.fog,_t=O.isMeshStandardMaterial||O.isMeshLambertMaterial||O.isMeshPhongMaterial?R.environment:null,ut=nt===null?L.outputColorSpace:nt.isXRRenderTarget===!0?nt.texture.colorSpace:Ht.workingColorSpace,xt=O.isMeshStandardMaterial||O.isMeshLambertMaterial&&!O.envMap||O.isMeshPhongMaterial&&!O.envMap,Mt=it.get(O.envMap||_t,xt),Ut=O.vertexColors===!0&&!!V.attributes.color&&V.attributes.color.itemSize===4,Bt=!!V.attributes.tangent&&(!!O.normalMap||O.anisotropy>0),vt=!!V.morphAttributes.position,Zt=!!V.morphAttributes.normal,de=!!V.morphAttributes.color;let ae=on;O.toneMapped&&(nt===null||nt.isXRRenderTarget===!0)&&(ae=L.toneMapping);const te=V.morphAttributes.position||V.morphAttributes.normal||V.morphAttributes.color,ye=te!==void 0?te.length:0,pt=z.get(O),Re=y.state.lights;if(Xt===!0&&(ie===!0||g!==j)){const se=g===j&&O.id===X;yt.setState(O,g,se)}let kt=!1;O.version===pt.__version?(pt.needsLights&&pt.lightsStateVersion!==Re.state.version||pt.outputColorSpace!==ut||B.isBatchedMesh&&pt.batching===!1||!B.isBatchedMesh&&pt.batching===!0||B.isBatchedMesh&&pt.batchingColor===!0&&B._colorsTexture===null||B.isBatchedMesh&&pt.batchingColor===!1&&B._colorsTexture!==null||B.isInstancedMesh&&pt.instancing===!1||!B.isInstancedMesh&&pt.instancing===!0||B.isSkinnedMesh&&pt.skinning===!1||!B.isSkinnedMesh&&pt.skinning===!0||B.isInstancedMesh&&pt.instancingColor===!0&&B.instanceColor===null||B.isInstancedMesh&&pt.instancingColor===!1&&B.instanceColor!==null||B.isInstancedMesh&&pt.instancingMorph===!0&&B.morphTexture===null||B.isInstancedMesh&&pt.instancingMorph===!1&&B.morphTexture!==null||pt.envMap!==Mt||O.fog===!0&&pt.fog!==dt||pt.numClippingPlanes!==void 0&&(pt.numClippingPlanes!==yt.numPlanes||pt.numIntersection!==yt.numIntersection)||pt.vertexAlphas!==Ut||pt.vertexTangents!==Bt||pt.morphTargets!==vt||pt.morphNormals!==Zt||pt.morphColors!==de||pt.toneMapping!==ae||pt.morphTargetsCount!==ye||!!pt.lightProbeGrid!=y.state.lightProbeGridArray.length>0)&&(kt=!0):(kt=!0,pt.__version=O.version);let ze=pt.currentProgram;kt===!0&&(ze=Wi(O,R,B),G&&O.isNodeMaterial&&G.onUpdateProgram(O,ze,pt));let en=!1,yn=!1,qn=!1;const jt=ze.getUniforms(),he=pt.uniforms;if(f.useProgram(ze.program)&&(en=!0,yn=!0,qn=!0),O.id!==X&&(X=O.id,yn=!0),pt.needsLights){const se=Yl(y.state.lightProbeGridArray,B);pt.lightProbeGrid!==se&&(pt.lightProbeGrid=se,yn=!0)}if(en||j!==g){f.buffers.depth.getReversed()&&g.reversedDepth!==!0&&(g._reversedDepth=!0,g.updateProjectionMatrix()),jt.setValue(D,"projectionMatrix",g.projectionMatrix),jt.setValue(D,"viewMatrix",g.matrixWorldInverse);const Tn=jt.map.cameraPosition;Tn!==void 0&&Tn.setValue(D,oe.setFromMatrixPosition(g.matrixWorld)),E.logarithmicDepthBuffer&&jt.setValue(D,"logDepthBufFC",2/(Math.log(g.far+1)/Math.LN2)),(O.isMeshPhongMaterial||O.isMeshToonMaterial||O.isMeshLambertMaterial||O.isMeshBasicMaterial||O.isMeshStandardMaterial||O.isShaderMaterial)&&jt.setValue(D,"isOrthographic",g.isOrthographicCamera===!0),j!==g&&(j=g,yn=!0,qn=!0)}if(pt.needsLights&&(Re.state.sunShadowMap.length>0&&jt.setValue(D,"sunShadowMap",Re.state.sunShadowMap,W),Re.state.directionalShadowMap.length>0&&jt.setValue(D,"directionalShadowMap",Re.state.directionalShadowMap,W),Re.state.spotShadowMap.length>0&&jt.setValue(D,"spotShadowMap",Re.state.spotShadowMap,W),Re.state.pointShadowMap.length>0&&jt.setValue(D,"pointShadowMap",Re.state.pointShadowMap,W)),B.isSkinnedMesh){jt.setOptional(D,B,"bindMatrix"),jt.setOptional(D,B,"bindMatrixInverse");const se=B.skeleton;se&&(se.boneTexture===null&&se.computeBoneTexture(),jt.setValue(D,"boneTexture",se.boneTexture,W))}B.isBatchedMesh&&(jt.setOptional(D,B,"batchingTexture"),jt.setValue(D,"batchingTexture",B._matricesTexture,W),jt.setOptional(D,B,"batchingIdTexture"),jt.setValue(D,"batchingIdTexture",B._indirectTexture,W),jt.setOptional(D,B,"batchingColorTexture"),B._colorsTexture!==null&&jt.setValue(D,"batchingColorTexture",B._colorsTexture,W));const bn=V.morphAttributes;if((bn.position!==void 0||bn.normal!==void 0||bn.color!==void 0)&&C.update(B,V,ze),(yn||pt.receiveShadow!==B.receiveShadow)&&(pt.receiveShadow=B.receiveShadow,jt.setValue(D,"receiveShadow",B.receiveShadow)),(O.isMeshStandardMaterial||O.isMeshLambertMaterial||O.isMeshPhongMaterial)&&O.envMap===null&&R.environment!==null&&(he.envMapIntensity.value=R.environmentIntensity),he.dfgLUT!==void 0&&(he.dfgLUT.value=Im()),yn){if(jt.setValue(D,"toneMappingExposure",L.toneMappingExposure),pt.needsLights&&Zl(he,qn),dt&&O.fog===!0&&Et.refreshFogUniforms(he,dt),Et.refreshMaterialUniforms(he,O,Q,Y,y.state.transmissionRenderTarget[g.id]),pt.needsLights&&pt.lightProbeGrid){const se=pt.lightProbeGrid;he.probesSH.value=se.texture,he.probesMin.value.copy(se.boundingBox.min),he.probesMax.value.copy(se.boundingBox.max),he.probesResolution.value.copy(se.resolution)}wr.upload(D,Ha(pt),he,W)}if(O.isShaderMaterial&&O.uniformsNeedUpdate===!0&&(wr.upload(D,Ha(pt),he,W),O.uniformsNeedUpdate=!1),O.isSpriteMaterial&&jt.setValue(D,"center",B.center),jt.setValue(D,"modelViewMatrix",B.modelViewMatrix),jt.setValue(D,"normalMatrix",B.normalMatrix),jt.setValue(D,"modelMatrix",B.matrixWorld),O.uniformsGroups!==void 0){const se=O.uniformsGroups;for(let Tn=0,Zn=se.length;Tn<Zn;Tn++){const Wa=se[Tn];tt.update(Wa,ze),tt.bind(Wa,ze)}}return ze}function Zl(g,R){g.ambientLightColor.needsUpdate=R,g.lightProbe.needsUpdate=R,g.sunLights.needsUpdate=R,g.sunLightShadows.needsUpdate=R,g.directionalLights.needsUpdate=R,g.directionalLightShadows.needsUpdate=R,g.pointLights.needsUpdate=R,g.pointLightShadows.needsUpdate=R,g.spotLights.needsUpdate=R,g.spotLightShadows.needsUpdate=R,g.rectAreaLights.needsUpdate=R,g.hemisphereLights.needsUpdate=R}function Kl(g){return g.isMeshLambertMaterial||g.isMeshToonMaterial||g.isMeshPhongMaterial||g.isMeshStandardMaterial||g.isShadowMaterial||g.isShaderMaterial&&g.lights===!0}this.getActiveCubeFace=function(){return K},this.getActiveMipmapLevel=function(){return Z},this.getRenderTarget=function(){return nt},this.setRenderTargetTextures=function(g,R,V){const O=z.get(g);O.__autoAllocateDepthBuffer=g.resolveDepthBuffer===!1,O.__autoAllocateDepthBuffer===!1&&(O.__useRenderToTexture=!1),z.get(g.texture).__webglTexture=R,z.get(g.depthTexture).__webglTexture=O.__autoAllocateDepthBuffer?void 0:V,O.__hasExternalTextures=!0},this.setRenderTargetFramebuffer=function(g,R){const V=z.get(g);V.__webglFramebuffer=R,V.__useDefaultFramebuffer=R===void 0},this.setRenderTarget=function(g,R=0,V=0){nt=g,K=R,Z=V;let O=null,B=!1,dt=!1;if(g){const ut=z.get(g);if(ut.__useDefaultFramebuffer!==void 0){f.bindFramebuffer(D.FRAMEBUFFER,ut.__webglFramebuffer),et.copy(g.viewport),At.copy(g.scissor),bt=g.scissorTest,f.viewport(et),f.scissor(At),f.setScissorTest(bt),X=-1;return}else if(ut.__webglFramebuffer===void 0)W.setupRenderTarget(g);else if(ut.__hasExternalTextures)W.rebindTextures(g,z.get(g.texture).__webglTexture,z.get(g.depthTexture).__webglTexture);else if(g.depthBuffer){const Ut=g.depthTexture;if(ut.__boundDepthTexture!==Ut){if(Ut!==null&&z.has(Ut)&&(g.width!==Ut.image.width||g.height!==Ut.image.height))throw new Error("THREE.WebGLRenderer: Attached DepthTexture is initialized to the incorrect size.");W.setupDepthRenderbuffer(g)}}const xt=g.texture;(xt.isData3DTexture||xt.isDataArrayTexture||xt.isCompressedArrayTexture)&&(dt=!0);const Mt=z.get(g).__webglFramebuffer;g.isWebGLCubeRenderTarget?(Array.isArray(Mt[R])?O=Mt[R][V]:O=Mt[R],B=!0):g.samples>0&&W.useMultisampledRTT(g)===!1?O=z.get(g).__webglMultisampledFramebuffer:Array.isArray(Mt)?O=Mt[V]:O=Mt,et.copy(g.viewport),At.copy(g.scissor),bt=g.scissorTest}else et.copy(mt).multiplyScalar(Q).floor(),At.copy(Ft).multiplyScalar(Q).floor(),bt=pe;if(V!==0&&(O=k),f.bindFramebuffer(D.FRAMEBUFFER,O)&&f.drawBuffers(g,O),f.viewport(et),f.scissor(At),f.setScissorTest(bt),B){const ut=z.get(g.texture);D.framebufferTexture2D(D.FRAMEBUFFER,D.COLOR_ATTACHMENT0,D.TEXTURE_CUBE_MAP_POSITIVE_X+R,ut.__webglTexture,V)}else if(dt){const ut=R;for(let xt=0;xt<g.textures.length;xt++){const Mt=z.get(g.textures[xt]);D.framebufferTextureLayer(D.FRAMEBUFFER,D.COLOR_ATTACHMENT0+xt,Mt.__webglTexture,V,ut)}}else if(g!==null&&V!==0){const ut=z.get(g.texture);D.framebufferTexture2D(D.FRAMEBUFFER,D.COLOR_ATTACHMENT0,D.TEXTURE_2D,ut.__webglTexture,V)}X=-1};function ka(g){const R=z.get(g);return(R.__readFormat!==g.format||R.__readType!==g.type)&&(R.__readFormat=g.format,R.__readType=g.type,R.__formatReadable=E.textureFormatReadable(g.format),R.__typeReadable=E.textureTypeReadable(g.type)),R}this.readRenderTargetPixels=function(g,R,V,O,B,dt,_t,ut=0){if(!(g&&g.isWebGLRenderTarget)){Wt("WebGLRenderer.readRenderTargetPixels: renderTarget is not THREE.WebGLRenderTarget.");return}let xt=z.get(g).__webglFramebuffer;if(g.isWebGLCubeRenderTarget&&_t!==void 0&&(xt=xt[_t]),xt){f.bindFramebuffer(D.FRAMEBUFFER,xt);try{const Mt=g.textures[ut],Ut=Mt.format,Bt=Mt.type;g.textures.length>1&&D.readBuffer(D.COLOR_ATTACHMENT0+ut);const vt=ka(Mt);if(vt.__formatReadable===!1){Wt("WebGLRenderer.readRenderTargetPixels: renderTarget is not in RGBA or implementation defined format.");return}if(vt.__typeReadable===!1){Wt("WebGLRenderer.readRenderTargetPixels: renderTarget is not in UnsignedByteType or implementation defined type.");return}R>=0&&R<=g.width-O&&V>=0&&V<=g.height-B&&D.readPixels(R,V,O,B,lt.convert(Ut),lt.convert(Bt),dt)}finally{const Mt=nt!==null?z.get(nt).__webglFramebuffer:null;f.bindFramebuffer(D.FRAMEBUFFER,Mt)}}},this.readRenderTargetPixelsAsync=async function(g,R,V,O,B,dt,_t,ut=0){if(!(g&&g.isWebGLRenderTarget))throw new Error("THREE.WebGLRenderer.readRenderTargetPixels: renderTarget is not THREE.WebGLRenderTarget.");let xt=z.get(g).__webglFramebuffer;if(g.isWebGLCubeRenderTarget&&_t!==void 0&&(xt=xt[_t]),xt)if(R>=0&&R<=g.width-O&&V>=0&&V<=g.height-B){f.bindFramebuffer(D.FRAMEBUFFER,xt);const Mt=g.textures[ut],Ut=Mt.format,Bt=Mt.type;g.textures.length>1&&D.readBuffer(D.COLOR_ATTACHMENT0+ut);const vt=ka(Mt);if(vt.__formatReadable===!1)throw new Error("THREE.WebGLRenderer.readRenderTargetPixelsAsync: renderTarget is not in RGBA or implementation defined format.");if(vt.__typeReadable===!1)throw new Error("THREE.WebGLRenderer.readRenderTargetPixelsAsync: renderTarget is not in UnsignedByteType or implementation defined type.");const Zt=D.createBuffer();D.bindBuffer(D.PIXEL_PACK_BUFFER,Zt),D.bufferData(D.PIXEL_PACK_BUFFER,dt.byteLength,D.STREAM_READ),D.readPixels(R,V,O,B,lt.convert(Ut),lt.convert(Bt),0),D.bindBuffer(D.PIXEL_PACK_BUFFER,null);const de=nt!==null?z.get(nt).__webglFramebuffer:null;f.bindFramebuffer(D.FRAMEBUFFER,de);const ae=D.fenceSync(D.SYNC_GPU_COMMANDS_COMPLETE,0);return D.flush(),await Dc(D,ae,4),D.bindBuffer(D.PIXEL_PACK_BUFFER,Zt),D.getBufferSubData(D.PIXEL_PACK_BUFFER,0,dt),D.bindBuffer(D.PIXEL_PACK_BUFFER,null),D.deleteBuffer(Zt),D.deleteSync(ae),dt}else throw new Error("THREE.WebGLRenderer.readRenderTargetPixelsAsync: requested read bounds are out of range.")},this.copyFramebufferToTexture=function(g,R=null,V=0){const O=Math.pow(2,-V),B=Math.floor(g.image.width*O),dt=Math.floor(g.image.height*O),_t=R!==null?R.x:0,ut=R!==null?R.y:0;W.setTexture2D(g,0),D.copyTexSubImage2D(D.TEXTURE_2D,V,0,0,_t,ut,B,dt),f.unbindTexture()},this.copyTextureToTexture=function(g,R,V=null,O=null,B=0,dt=0){let _t,ut,xt,Mt,Ut,Bt,vt,Zt,de;const ae=g.isCompressedTexture?g.mipmaps[dt]:g.image;if(V!==null)_t=V.max.x-V.min.x,ut=V.max.y-V.min.y,xt=V.isBox3?V.max.z-V.min.z:1,Mt=V.min.x,Ut=V.min.y,Bt=V.isBox3?V.min.z:0;else{const he=Math.pow(2,-B);_t=Math.floor(ae.width*he),ut=Math.floor(ae.height*he),g.isDataArrayTexture?xt=ae.depth:g.isData3DTexture?xt=Math.floor(ae.depth*he):xt=1,Mt=0,Ut=0,Bt=0}O!==null?(vt=O.x,Zt=O.y,de=O.z):(vt=0,Zt=0,de=0);const te=lt.convert(R.format),ye=lt.convert(R.type);let pt;R.isData3DTexture?(W.setTexture3D(R,0),pt=D.TEXTURE_3D):R.isDataArrayTexture||R.isCompressedArrayTexture?(W.setTexture2DArray(R,0),pt=D.TEXTURE_2D_ARRAY):(W.setTexture2D(R,0),pt=D.TEXTURE_2D),f.activeTexture(D.TEXTURE0),f.pixelStorei(D.UNPACK_FLIP_Y_WEBGL,R.flipY),f.pixelStorei(D.UNPACK_PREMULTIPLY_ALPHA_WEBGL,R.premultiplyAlpha),f.pixelStorei(D.UNPACK_ALIGNMENT,R.unpackAlignment);const Re=f.getParameter(D.UNPACK_ROW_LENGTH),kt=f.getParameter(D.UNPACK_IMAGE_HEIGHT),ze=f.getParameter(D.UNPACK_SKIP_PIXELS),en=f.getParameter(D.UNPACK_SKIP_ROWS),yn=f.getParameter(D.UNPACK_SKIP_IMAGES);f.pixelStorei(D.UNPACK_ROW_LENGTH,ae.width),f.pixelStorei(D.UNPACK_IMAGE_HEIGHT,ae.height),f.pixelStorei(D.UNPACK_SKIP_PIXELS,Mt),f.pixelStorei(D.UNPACK_SKIP_ROWS,Ut),f.pixelStorei(D.UNPACK_SKIP_IMAGES,Bt);const qn=g.isDataArrayTexture||g.isData3DTexture,jt=R.isDataArrayTexture||R.isData3DTexture;if(g.isDepthTexture){const he=z.get(g),bn=z.get(R),se=z.get(he.__renderTarget),Tn=z.get(bn.__renderTarget);f.bindFramebuffer(D.READ_FRAMEBUFFER,se.__webglFramebuffer),f.bindFramebuffer(D.DRAW_FRAMEBUFFER,Tn.__webglFramebuffer);for(let Zn=0;Zn<xt;Zn++)qn&&(D.framebufferTextureLayer(D.READ_FRAMEBUFFER,D.COLOR_ATTACHMENT0,z.get(g).__webglTexture,B,Bt+Zn),D.framebufferTextureLayer(D.DRAW_FRAMEBUFFER,D.COLOR_ATTACHMENT0,z.get(R).__webglTexture,dt,de+Zn)),D.blitFramebuffer(Mt,Ut,_t,ut,vt,Zt,_t,ut,D.DEPTH_BUFFER_BIT,D.NEAREST);f.bindFramebuffer(D.READ_FRAMEBUFFER,null),f.bindFramebuffer(D.DRAW_FRAMEBUFFER,null)}else if(B!==0||g.isRenderTargetTexture||z.has(g)){const he=z.get(g),bn=z.get(R);f.bindFramebuffer(D.READ_FRAMEBUFFER,U),f.bindFramebuffer(D.DRAW_FRAMEBUFFER,H);for(let se=0;se<xt;se++)qn?D.framebufferTextureLayer(D.READ_FRAMEBUFFER,D.COLOR_ATTACHMENT0,he.__webglTexture,B,Bt+se):D.framebufferTexture2D(D.READ_FRAMEBUFFER,D.COLOR_ATTACHMENT0,D.TEXTURE_2D,he.__webglTexture,B),jt?D.framebufferTextureLayer(D.DRAW_FRAMEBUFFER,D.COLOR_ATTACHMENT0,bn.__webglTexture,dt,de+se):D.framebufferTexture2D(D.DRAW_FRAMEBUFFER,D.COLOR_ATTACHMENT0,D.TEXTURE_2D,bn.__webglTexture,dt),B!==0?D.blitFramebuffer(Mt,Ut,_t,ut,vt,Zt,_t,ut,D.COLOR_BUFFER_BIT,D.NEAREST):jt?D.copyTexSubImage3D(pt,dt,vt,Zt,de+se,Mt,Ut,_t,ut):D.copyTexSubImage2D(pt,dt,vt,Zt,Mt,Ut,_t,ut);f.bindFramebuffer(D.READ_FRAMEBUFFER,null),f.bindFramebuffer(D.DRAW_FRAMEBUFFER,null)}else jt?g.isDataTexture||g.isData3DTexture?D.texSubImage3D(pt,dt,vt,Zt,de,_t,ut,xt,te,ye,ae.data):R.isCompressedArrayTexture?D.compressedTexSubImage3D(pt,dt,vt,Zt,de,_t,ut,xt,te,ae.data):D.texSubImage3D(pt,dt,vt,Zt,de,_t,ut,xt,te,ye,ae):g.isDataTexture?D.texSubImage2D(D.TEXTURE_2D,dt,vt,Zt,_t,ut,te,ye,ae.data):g.isCompressedTexture?D.compressedTexSubImage2D(D.TEXTURE_2D,dt,vt,Zt,ae.width,ae.height,te,ae.data):D.texSubImage2D(D.TEXTURE_2D,dt,vt,Zt,_t,ut,te,ye,ae);f.pixelStorei(D.UNPACK_ROW_LENGTH,Re),f.pixelStorei(D.UNPACK_IMAGE_HEIGHT,kt),f.pixelStorei(D.UNPACK_SKIP_PIXELS,ze),f.pixelStorei(D.UNPACK_SKIP_ROWS,en),f.pixelStorei(D.UNPACK_SKIP_IMAGES,yn),dt===0&&R.generateMipmaps&&D.generateMipmap(pt),f.unbindTexture()},this.initRenderTarget=function(g){z.get(g).__webglFramebuffer===void 0&&W.setupRenderTarget(g)},this.initTexture=function(g){g.isCubeTexture?W.setTextureCube(g,0):g.isData3DTexture?W.setTexture3D(g,0):g.isDataArrayTexture||g.isCompressedArrayTexture?W.setTexture2DArray(g,0):W.setTexture2D(g,0),f.unbindTexture()},this.resetState=function(){K=0,Z=0,nt=null,f.reset(),ft.reset()},typeof __THREE_DEVTOOLS__<"u"&&__THREE_DEVTOOLS__.dispatchEvent(new CustomEvent("observe",{detail:this}))}get coordinateSystem(){return an}get outputColorSpace(){return this._outputColorSpace}set outputColorSpace(t){this._outputColorSpace=t;const e=this.getContext();e.drawingBufferColorSpace=Ht._getDrawingBufferColorSpace(t),e.unpackColorSpace=Ht._getUnpackColorSpace()}}const tl={type:"change"},Ca={type:"start"},Vl={type:"end"},Mr=new Br,el=new _n,Um=Math.cos(70*Kc.DEG2RAD),me=new I,Ie=2*Math.PI,Qt={NONE:-1,ROTATE:0,DOLLY:1,PAN:2,TOUCH_ROTATE:3,TOUCH_PAN:4,TOUCH_DOLLY_PAN:5,TOUCH_DOLLY_ROTATE:6},bs=1e-6;class l_ extends Nh{constructor(t,e=null){super(t,e),this.state=Qt.NONE,this.target=new I,this.cursor=new I,this.minDistance=0,this.maxDistance=1/0,this.minZoom=0,this.maxZoom=1/0,this.minTargetRadius=0,this.maxTargetRadius=1/0,this.minPolarAngle=0,this.maxPolarAngle=Math.PI,this.minAzimuthAngle=-1/0,this.maxAzimuthAngle=1/0,this.enableDamping=!1,this.dampingFactor=.05,this.enableZoom=!0,this.zoomSpeed=1,this.enableRotate=!0,this.rotateSpeed=1,this.keyRotateSpeed=1,this.enablePan=!0,this.panSpeed=1,this.screenSpacePanning=!0,this.keyPanSpeed=7,this.zoomToCursor=!1,this.autoRotate=!1,this.autoRotateSpeed=2,this.keys={LEFT:"ArrowLeft",UP:"ArrowUp",RIGHT:"ArrowRight",BOTTOM:"ArrowDown"},this.mouseButtons={LEFT:pi.ROTATE,MIDDLE:pi.DOLLY,RIGHT:pi.PAN},this.touches={ONE:di.ROTATE,TWO:di.DOLLY_PAN},this.target0=this.target.clone(),this.position0=this.object.position.clone(),this.zoom0=this.object.zoom,this._cursorStyle="auto",this._domElementKeyEvents=null,this._lastPosition=new I,this._lastQuaternion=new Ln,this._lastTargetPosition=new I,this._quat=new Ln().setFromUnitVectors(t.up,new I(0,1,0)),this._quatInverse=this._quat.clone().invert(),this._spherical=new Ro,this._sphericalDelta=new Ro,this._scale=1,this._panOffset=new I,this._rotateStart=new Ct,this._rotateEnd=new Ct,this._rotateDelta=new Ct,this._panStart=new Ct,this._panEnd=new Ct,this._panDelta=new Ct,this._dollyStart=new Ct,this._dollyEnd=new Ct,this._dollyDelta=new Ct,this._dollyDirection=new I,this._mouse=new Ct,this._performCursorZoom=!1,this._pointers=[],this._pointerPositions={},this._controlActive=!1,this._onPointerMove=Fm.bind(this),this._onPointerDown=Nm.bind(this),this._onPointerUp=Om.bind(this),this._onContextMenu=Wm.bind(this),this._onMouseWheel=Gm.bind(this),this._onKeyDown=Hm.bind(this),this._onTouchStart=Vm.bind(this),this._onTouchMove=km.bind(this),this._onMouseDown=Bm.bind(this),this._onMouseMove=zm.bind(this),this._interceptControlDown=Xm.bind(this),this._interceptControlUp=Ym.bind(this),this.domElement!==null&&this.connect(this.domElement),this.update()}set cursorStyle(t){this._cursorStyle=t,t==="grab"?this.domElement.style.cursor="grab":this.domElement.style.cursor="auto"}get cursorStyle(){return this._cursorStyle}connect(t){super.connect(t),this.domElement.addEventListener("pointerdown",this._onPointerDown),this.domElement.addEventListener("pointercancel",this._onPointerUp),this.domElement.addEventListener("contextmenu",this._onContextMenu),this.domElement.addEventListener("wheel",this._onMouseWheel,{passive:!1}),this.domElement.getRootNode().addEventListener("keydown",this._interceptControlDown,{passive:!0,capture:!0}),this.domElement.style.touchAction="none"}disconnect(){this.state=Qt.NONE,this.domElement.removeEventListener("pointerdown",this._onPointerDown),this.domElement.ownerDocument.removeEventListener("pointermove",this._onPointerMove),this.domElement.ownerDocument.removeEventListener("pointerup",this._onPointerUp),this.domElement.removeEventListener("pointercancel",this._onPointerUp),this.domElement.removeEventListener("wheel",this._onMouseWheel),this.domElement.removeEventListener("contextmenu",this._onContextMenu),this.stopListenToKeyEvents();const t=this.domElement.getRootNode();t.removeEventListener("keydown",this._interceptControlDown,{capture:!0}),t.removeEventListener("keyup",this._interceptControlUp,{capture:!0}),this._controlActive=!1,this._pointers.length=0,this._pointerPositions={},this.domElement.style.touchAction="",this.domElement.style.cursor="auto"}dispose(){this.disconnect()}getPolarAngle(){return this._spherical.phi}getAzimuthalAngle(){return this._spherical.theta}getDistance(){return this.object.position.distanceTo(this.target)}listenToKeyEvents(t){t.addEventListener("keydown",this._onKeyDown),this._domElementKeyEvents=t}stopListenToKeyEvents(){this._domElementKeyEvents!==null&&(this._domElementKeyEvents.removeEventListener("keydown",this._onKeyDown),this._domElementKeyEvents=null)}saveState(){this.target0.copy(this.target),this.position0.copy(this.object.position),this.zoom0=this.object.zoom}reset(){this.target.copy(this.target0),this.object.position.copy(this.position0),this.object.zoom=this.zoom0,this.object.updateProjectionMatrix(),this.dispatchEvent(tl),this.update(),this.state=Qt.NONE}pan(t,e){this._pan(t,e),this.update()}dollyIn(t){this._dollyIn(t),this.update()}dollyOut(t){this._dollyOut(t),this.update()}rotateLeft(t){this._rotateLeft(t),this.update()}rotateUp(t){this._rotateUp(t),this.update()}update(t=null){const e=this.object.position;me.copy(e).sub(this.target),me.applyQuaternion(this._quat),this._spherical.setFromVector3(me),this.autoRotate&&this.state===Qt.NONE&&this._rotateLeft(this._getAutoRotationAngle(t)),this.enableDamping?(this._spherical.theta+=this._sphericalDelta.theta*this.dampingFactor,this._spherical.phi+=this._sphericalDelta.phi*this.dampingFactor):(this._spherical.theta+=this._sphericalDelta.theta,this._spherical.phi+=this._sphericalDelta.phi);let n=this.minAzimuthAngle,r=this.maxAzimuthAngle;isFinite(n)&&isFinite(r)&&(n<-Math.PI?n+=Ie:n>Math.PI&&(n-=Ie),r<-Math.PI?r+=Ie:r>Math.PI&&(r-=Ie),n<=r?this._spherical.theta=Math.max(n,Math.min(r,this._spherical.theta)):this._spherical.theta=this._spherical.theta>(n+r)/2?Math.max(n,this._spherical.theta):Math.min(r,this._spherical.theta)),this._spherical.phi=Math.max(this.minPolarAngle,Math.min(this.maxPolarAngle,this._spherical.phi)),this._spherical.makeSafe(),this.enableDamping===!0?this.target.addScaledVector(this._panOffset,this.dampingFactor):this.target.add(this._panOffset),this.target.sub(this.cursor),this.target.clampLength(this.minTargetRadius,this.maxTargetRadius),this.target.add(this.cursor);let s=!1;if(this.zoomToCursor&&this._performCursorZoom||this.object.isOrthographicCamera)this._spherical.radius=this._clampDistance(this._spherical.radius);else{const a=this._spherical.radius;this._spherical.radius=this._clampDistance(this._spherical.radius*this._scale),s=a!=this._spherical.radius}if(me.setFromSpherical(this._spherical),me.applyQuaternion(this._quatInverse),e.copy(this.target).add(me),this.object.lookAt(this.target),this.enableDamping===!0?(this._sphericalDelta.theta*=1-this.dampingFactor,this._sphericalDelta.phi*=1-this.dampingFactor,this._panOffset.multiplyScalar(1-this.dampingFactor)):(this._sphericalDelta.set(0,0,0),this._panOffset.set(0,0,0)),this.zoomToCursor&&this._performCursorZoom){let a=null;if(this.object.isPerspectiveCamera){const o=me.length();a=this._clampDistance(o*this._scale);const c=o-a;this.object.position.addScaledVector(this._dollyDirection,c),this.object.updateMatrixWorld(),s=!!c}else if(this.object.isOrthographicCamera){const o=new I(this._mouse.x,this._mouse.y,0);o.unproject(this.object);const c=this.object.zoom;this.object.zoom=Math.max(this.minZoom,Math.min(this.maxZoom,this.object.zoom/this._scale)),this.object.updateProjectionMatrix(),s=c!==this.object.zoom;const l=new I(this._mouse.x,this._mouse.y,0);l.unproject(this.object),this.object.position.sub(l).add(o),this.object.updateMatrixWorld(),a=me.length()}else console.warn("WARNING: OrbitControls.js encountered an unknown camera type - zoom to cursor disabled."),this.zoomToCursor=!1;a!==null&&(this.screenSpacePanning?this.target.set(0,0,-1).transformDirection(this.object.matrix).multiplyScalar(a).add(this.object.position):(Mr.origin.copy(this.object.position),Mr.direction.set(0,0,-1).transformDirection(this.object.matrix),Math.abs(this.object.up.dot(Mr.direction))<Um?this.object.lookAt(this.target):(el.setFromNormalAndCoplanarPoint(this.object.up,this.target),Mr.intersectPlane(el,this.target))))}else if(this.object.isOrthographicCamera){const a=this.object.zoom;this.object.zoom=Math.max(this.minZoom,Math.min(this.maxZoom,this.object.zoom/this._scale)),a!==this.object.zoom&&(this.object.updateProjectionMatrix(),s=!0)}return this._scale=1,this._performCursorZoom=!1,s||this._lastPosition.distanceToSquared(this.object.position)>bs||8*(1-this._lastQuaternion.dot(this.object.quaternion))>bs||this._lastTargetPosition.distanceToSquared(this.target)>bs?(this.dispatchEvent(tl),this._lastPosition.copy(this.object.position),this._lastQuaternion.copy(this.object.quaternion),this._lastTargetPosition.copy(this.target),!0):!1}_getAutoRotationAngle(t){return t!==null?Ie/60*this.autoRotateSpeed*t:Ie/60/60*this.autoRotateSpeed}_getZoomScale(t){const e=Math.abs(t*.01);return Math.pow(.95,this.zoomSpeed*e)}_rotateLeft(t){this._sphericalDelta.theta-=t}_rotateUp(t){this._sphericalDelta.phi-=t}_panLeft(t,e){me.setFromMatrixColumn(e,0),me.multiplyScalar(-t),this._panOffset.add(me)}_panUp(t,e){this.screenSpacePanning===!0?me.setFromMatrixColumn(e,1):(me.setFromMatrixColumn(e,0),me.crossVectors(this.object.up,me)),me.multiplyScalar(t),this._panOffset.add(me)}_pan(t,e){const n=this.domElement;if(this.object.isPerspectiveCamera){const r=this.object.position;me.copy(r).sub(this.target);let s=me.length();s*=Math.tan(this.object.fov/2*Math.PI/180),this._panLeft(2*t*s/n.clientHeight,this.object.matrix),this._panUp(2*e*s/n.clientHeight,this.object.matrix)}else this.object.isOrthographicCamera?(this._panLeft(t*(this.object.right-this.object.left)/this.object.zoom/n.clientWidth,this.object.matrix),this._panUp(e*(this.object.top-this.object.bottom)/this.object.zoom/n.clientHeight,this.object.matrix)):(console.warn("WARNING: OrbitControls.js encountered an unknown camera type - pan disabled."),this.enablePan=!1)}_dollyOut(t){this.object.isPerspectiveCamera||this.object.isOrthographicCamera?this._scale/=t:(console.warn("WARNING: OrbitControls.js encountered an unknown camera type - dolly/zoom disabled."),this.enableZoom=!1)}_dollyIn(t){this.object.isPerspectiveCamera||this.object.isOrthographicCamera?this._scale*=t:(console.warn("WARNING: OrbitControls.js encountered an unknown camera type - dolly/zoom disabled."),this.enableZoom=!1)}_updateZoomParameters(t,e){if(!this.zoomToCursor)return;this._performCursorZoom=!0;const n=this.domElement.getBoundingClientRect(),r=t-n.left,s=e-n.top,a=n.width,o=n.height;this._mouse.x=r/a*2-1,this._mouse.y=-(s/o)*2+1,this._dollyDirection.set(this._mouse.x,this._mouse.y,1).unproject(this.object).sub(this.object.position).normalize()}_clampDistance(t){return Math.max(this.minDistance,Math.min(this.maxDistance,t))}_handleMouseDownRotate(t){this._rotateStart.set(t.clientX,t.clientY)}_handleMouseDownDolly(t){this._updateZoomParameters(t.clientX,t.clientX),this._dollyStart.set(t.clientX,t.clientY)}_handleMouseDownPan(t){this._panStart.set(t.clientX,t.clientY)}_handleMouseMoveRotate(t){this._rotateEnd.set(t.clientX,t.clientY),this._rotateDelta.subVectors(this._rotateEnd,this._rotateStart).multiplyScalar(this.rotateSpeed);const e=this.domElement;this._rotateLeft(Ie*this._rotateDelta.x/e.clientHeight),this._rotateUp(Ie*this._rotateDelta.y/e.clientHeight),this._rotateStart.copy(this._rotateEnd),this.update()}_handleMouseMoveDolly(t){this._dollyEnd.set(t.clientX,t.clientY),this._dollyDelta.subVectors(this._dollyEnd,this._dollyStart),this._dollyDelta.y>0?this._dollyOut(this._getZoomScale(this._dollyDelta.y)):this._dollyDelta.y<0&&this._dollyIn(this._getZoomScale(this._dollyDelta.y)),this._dollyStart.copy(this._dollyEnd),this.update()}_handleMouseMovePan(t){this._panEnd.set(t.clientX,t.clientY),this._panDelta.subVectors(this._panEnd,this._panStart).multiplyScalar(this.panSpeed),this._pan(this._panDelta.x,this._panDelta.y),this._panStart.copy(this._panEnd),this.update()}_handleMouseWheel(t){this._updateZoomParameters(t.clientX,t.clientY),t.deltaY<0?this._dollyIn(this._getZoomScale(t.deltaY)):t.deltaY>0&&this._dollyOut(this._getZoomScale(t.deltaY)),this.update()}_handleKeyDown(t){let e=!1;switch(t.code){case this.keys.UP:t.ctrlKey||t.metaKey||t.shiftKey?this.enableRotate&&this._rotateUp(Ie*this.keyRotateSpeed/this.domElement.clientHeight):this.enablePan&&this._pan(0,this.keyPanSpeed),e=!0;break;case this.keys.BOTTOM:t.ctrlKey||t.metaKey||t.shiftKey?this.enableRotate&&this._rotateUp(-Ie*this.keyRotateSpeed/this.domElement.clientHeight):this.enablePan&&this._pan(0,-this.keyPanSpeed),e=!0;break;case this.keys.LEFT:t.ctrlKey||t.metaKey||t.shiftKey?this.enableRotate&&this._rotateLeft(Ie*this.keyRotateSpeed/this.domElement.clientHeight):this.enablePan&&this._pan(this.keyPanSpeed,0),e=!0;break;case this.keys.RIGHT:t.ctrlKey||t.metaKey||t.shiftKey?this.enableRotate&&this._rotateLeft(-Ie*this.keyRotateSpeed/this.domElement.clientHeight):this.enablePan&&this._pan(-this.keyPanSpeed,0),e=!0;break}e&&(t.preventDefault(),this.update())}_handleTouchStartRotate(t){if(this._pointers.length===1)this._rotateStart.set(t.pageX,t.pageY);else{const e=this._getSecondPointerPosition(t),n=.5*(t.pageX+e.x),r=.5*(t.pageY+e.y);this._rotateStart.set(n,r)}}_handleTouchStartPan(t){if(this._pointers.length===1)this._panStart.set(t.pageX,t.pageY);else{const e=this._getSecondPointerPosition(t),n=.5*(t.pageX+e.x),r=.5*(t.pageY+e.y);this._panStart.set(n,r)}}_handleTouchStartDolly(t){const e=this._getSecondPointerPosition(t),n=t.pageX-e.x,r=t.pageY-e.y,s=Math.sqrt(n*n+r*r);this._dollyStart.set(0,s)}_handleTouchStartDollyPan(t){this.enableZoom&&this._handleTouchStartDolly(t),this.enablePan&&this._handleTouchStartPan(t)}_handleTouchStartDollyRotate(t){this.enableZoom&&this._handleTouchStartDolly(t),this.enableRotate&&this._handleTouchStartRotate(t)}_handleTouchMoveRotate(t){if(this._pointers.length==1)this._rotateEnd.set(t.pageX,t.pageY);else{const n=this._getSecondPointerPosition(t),r=.5*(t.pageX+n.x),s=.5*(t.pageY+n.y);this._rotateEnd.set(r,s)}this._rotateDelta.subVectors(this._rotateEnd,this._rotateStart).multiplyScalar(this.rotateSpeed);const e=this.domElement;this._rotateLeft(Ie*this._rotateDelta.x/e.clientHeight),this._rotateUp(Ie*this._rotateDelta.y/e.clientHeight),this._rotateStart.copy(this._rotateEnd)}_handleTouchMovePan(t){if(this._pointers.length===1)this._panEnd.set(t.pageX,t.pageY);else{const e=this._getSecondPointerPosition(t),n=.5*(t.pageX+e.x),r=.5*(t.pageY+e.y);this._panEnd.set(n,r)}this._panDelta.subVectors(this._panEnd,this._panStart).multiplyScalar(this.panSpeed),this._pan(this._panDelta.x,this._panDelta.y),this._panStart.copy(this._panEnd)}_handleTouchMoveDolly(t){const e=this._getSecondPointerPosition(t),n=t.pageX-e.x,r=t.pageY-e.y,s=Math.sqrt(n*n+r*r);this._dollyEnd.set(0,s),this._dollyDelta.set(0,Math.pow(this._dollyEnd.y/this._dollyStart.y,this.zoomSpeed)),this._dollyOut(this._dollyDelta.y),this._dollyStart.copy(this._dollyEnd);const a=(t.pageX+e.x)*.5,o=(t.pageY+e.y)*.5;this._updateZoomParameters(a,o)}_handleTouchMoveDollyPan(t){this.enableZoom&&this._handleTouchMoveDolly(t),this.enablePan&&this._handleTouchMovePan(t)}_handleTouchMoveDollyRotate(t){this.enableZoom&&this._handleTouchMoveDolly(t),this.enableRotate&&this._handleTouchMoveRotate(t)}_addPointer(t){this._pointers.push(t.pointerId)}_removePointer(t){delete this._pointerPositions[t.pointerId];for(let e=0;e<this._pointers.length;e++)if(this._pointers[e]==t.pointerId){this._pointers.splice(e,1);return}}_isTrackingPointer(t){for(let e=0;e<this._pointers.length;e++)if(this._pointers[e]==t.pointerId)return!0;return!1}_trackPointer(t){let e=this._pointerPositions[t.pointerId];e===void 0&&(e=new Ct,this._pointerPositions[t.pointerId]=e),e.set(t.pageX,t.pageY)}_getSecondPointerPosition(t){const e=t.pointerId===this._pointers[0]?this._pointers[1]:this._pointers[0];return this._pointerPositions[e]}_customWheelEvent(t){const e=t.deltaMode,n={clientX:t.clientX,clientY:t.clientY,deltaY:t.deltaY};switch(e){case 1:n.deltaY*=16;break;case 2:n.deltaY*=100;break}return t.ctrlKey&&!this._controlActive&&(n.deltaY*=10),n}}function Nm(i){this.enabled!==!1&&(this._pointers.length===0&&(this.domElement.setPointerCapture(i.pointerId),this.domElement.ownerDocument.addEventListener("pointermove",this._onPointerMove),this.domElement.ownerDocument.addEventListener("pointerup",this._onPointerUp)),!this._isTrackingPointer(i)&&(this._addPointer(i),i.pointerType==="touch"?this._onTouchStart(i):this._onMouseDown(i),this._cursorStyle==="grab"&&(this.domElement.style.cursor="grabbing")))}function Fm(i){this.enabled!==!1&&(i.pointerType==="touch"?this._onTouchMove(i):this._onMouseMove(i))}function Om(i){switch(this._removePointer(i),this._pointers.length){case 0:this.domElement.releasePointerCapture(i.pointerId),this.domElement.ownerDocument.removeEventListener("pointermove",this._onPointerMove),this.domElement.ownerDocument.removeEventListener("pointerup",this._onPointerUp),this.dispatchEvent(Vl),this.state=Qt.NONE,this._cursorStyle==="grab"&&(this.domElement.style.cursor="grab");break;case 1:const t=this._pointers[0],e=this._pointerPositions[t];this._onTouchStart({pointerId:t,pageX:e.x,pageY:e.y});break}}function Bm(i){let t;switch(i.button){case 0:t=this.mouseButtons.LEFT;break;case 1:t=this.mouseButtons.MIDDLE;break;case 2:t=this.mouseButtons.RIGHT;break;default:t=-1}switch(t){case pi.DOLLY:if(this.enableZoom===!1)return;this._handleMouseDownDolly(i),this.state=Qt.DOLLY;break;case pi.ROTATE:if(i.ctrlKey||i.metaKey||i.shiftKey){if(this.enablePan===!1)return;this._handleMouseDownPan(i),this.state=Qt.PAN}else{if(this.enableRotate===!1)return;this._handleMouseDownRotate(i),this.state=Qt.ROTATE}break;case pi.PAN:if(i.ctrlKey||i.metaKey||i.shiftKey){if(this.enableRotate===!1)return;this._handleMouseDownRotate(i),this.state=Qt.ROTATE}else{if(this.enablePan===!1)return;this._handleMouseDownPan(i),this.state=Qt.PAN}break;default:this.state=Qt.NONE}this.state!==Qt.NONE&&this.dispatchEvent(Ca)}function zm(i){switch(this.state){case Qt.ROTATE:if(this.enableRotate===!1)return;this._handleMouseMoveRotate(i);break;case Qt.DOLLY:if(this.enableZoom===!1)return;this._handleMouseMoveDolly(i);break;case Qt.PAN:if(this.enablePan===!1)return;this._handleMouseMovePan(i);break}}function Gm(i){this.enabled===!1||this.enableZoom===!1||this.state!==Qt.NONE||(i.preventDefault(),this.dispatchEvent(Ca),this._handleMouseWheel(this._customWheelEvent(i)),this.dispatchEvent(Vl))}function Hm(i){this.enabled!==!1&&this._handleKeyDown(i)}function Vm(i){switch(this._trackPointer(i),this._pointers.length){case 1:switch(this.touches.ONE){case di.ROTATE:if(this.enableRotate===!1)return;this._handleTouchStartRotate(i),this.state=Qt.TOUCH_ROTATE;break;case di.PAN:if(this.enablePan===!1)return;this._handleTouchStartPan(i),this.state=Qt.TOUCH_PAN;break;default:this.state=Qt.NONE}break;case 2:switch(this.touches.TWO){case di.DOLLY_PAN:if(this.enableZoom===!1&&this.enablePan===!1)return;this._handleTouchStartDollyPan(i),this.state=Qt.TOUCH_DOLLY_PAN;break;case di.DOLLY_ROTATE:if(this.enableZoom===!1&&this.enableRotate===!1)return;this._handleTouchStartDollyRotate(i),this.state=Qt.TOUCH_DOLLY_ROTATE;break;default:this.state=Qt.NONE}break;default:this.state=Qt.NONE}this.state!==Qt.NONE&&this.dispatchEvent(Ca)}function km(i){switch(this._trackPointer(i),this.state){case Qt.TOUCH_ROTATE:if(this.enableRotate===!1)return;this._handleTouchMoveRotate(i),this.update();break;case Qt.TOUCH_PAN:if(this.enablePan===!1)return;this._handleTouchMovePan(i),this.update();break;case Qt.TOUCH_DOLLY_PAN:if(this.enableZoom===!1&&this.enablePan===!1)return;this._handleTouchMoveDollyPan(i),this.update();break;case Qt.TOUCH_DOLLY_ROTATE:if(this.enableZoom===!1&&this.enableRotate===!1)return;this._handleTouchMoveDollyRotate(i),this.update();break;default:this.state=Qt.NONE}}function Wm(i){this.enabled!==!1&&i.preventDefault()}function Xm(i){i.key==="Control"&&(this._controlActive=!0,this.domElement.getRootNode().addEventListener("keyup",this._interceptControlUp,{passive:!0,capture:!0}))}function Ym(i){i.key==="Control"&&(this._controlActive=!1,this.domElement.getRootNode().removeEventListener("keyup",this._interceptControlUp,{passive:!0,capture:!0}))}export{Ya as A,Yn as B,Gt as C,e_ as D,Jm as E,we as F,Zi as G,t_ as H,i_ as I,a_ as L,je as M,l_ as O,qe as P,Ln as Q,s_ as R,Si as S,yh as U,I as V,Qm as W,r_ as a,Al as b,hn as c,sn as d,Ct as e,ht as f,ce as g,ee as h,Kc as i,Zm as j,yl as k,o_ as l,Vi as m,wl as n,$m as o,vh as p,jm as q,Ne as r,Qe as s,Km as t,qm as u,n_ as v,Dl as w};
