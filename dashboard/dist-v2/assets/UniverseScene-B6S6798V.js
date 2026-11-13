var Pe=Object.defineProperty;var Be=(f,e,t)=>e in f?Pe(f,e,{enumerable:!0,configurable:!0,writable:!0,value:t}):f[e]=t;var u=(f,e,t)=>Be(f,typeof e!="symbol"?e+"":e,t);import{I as Oe,F as ae,a as re,b as R,W as Ce,B as F,S as Ae,V as p,c as Ie,d as K,U as Le,e as ce,f as Y,M as Z,g as $,L as Te,h as le,i as ke,j as Re,G as ee,k as Fe,R as He,C as P,P as je,l as We,O as Ne,H as $e,D as pe,m as j,n as Ge,o as W,E as te,p as N,q as ne,r as ge,s as ye,A as we,t as Ve,u as qe,Q as ve}from"./OrbitControls-CXezoCud.js";import{l as Xe,s as Qe,h as Je,f as Ke,a as be,c as xe}from"./index.v2-D7xPqbuB.js";import"./react-Brj5lhMG.js";import"./tanstack-BmZ42l7s.js";import"./radix-CQHGBvGr.js";const Se=new F,q=new p;class Ue extends Oe{constructor(){super(),this.isLineSegmentsGeometry=!0,this.type="LineSegmentsGeometry";const e=[-1,2,0,1,2,0,-1,1,0,1,1,0,-1,0,0,1,0,0,-1,-1,0,1,-1,0],t=[-1,2,1,2,-1,1,1,1,-1,-1,1,-1,-1,-2,1,-2],n=[0,2,1,2,3,1,2,4,3,4,5,3,4,6,5,6,7,5];this.setIndex(n),this.setAttribute("position",new ae(e,3)),this.setAttribute("uv",new ae(t,2))}applyMatrix4(e){const t=this.attributes.instanceStart,n=this.attributes.instanceEnd;return t!==void 0&&(t.applyMatrix4(e),n.applyMatrix4(e),t.needsUpdate=!0),this.boundingBox!==null&&this.computeBoundingBox(),this.boundingSphere!==null&&this.computeBoundingSphere(),this}setPositions(e){let t;e instanceof Float32Array?t=e:Array.isArray(e)&&(t=new Float32Array(e));const n=new re(t,6,1);return this.setAttribute("instanceStart",new R(n,3,0)),this.setAttribute("instanceEnd",new R(n,3,3)),this.instanceCount=this.attributes.instanceStart.count,this.computeBoundingBox(),this.computeBoundingSphere(),this}setColors(e){let t;e instanceof Float32Array?t=e:Array.isArray(e)&&(t=new Float32Array(e));const n=new re(t,6,1);return this.setAttribute("instanceColorStart",new R(n,3,0)),this.setAttribute("instanceColorEnd",new R(n,3,3)),this}fromWireframeGeometry(e){return this.setPositions(e.attributes.position.array),this}fromEdgesGeometry(e){return this.setPositions(e.attributes.position.array),this}fromMesh(e){return this.fromWireframeGeometry(new Ce(e.geometry)),this}fromLineSegments(e){const t=e.geometry;return this.setPositions(t.attributes.position.array),this}computeBoundingBox(){this.boundingBox===null&&(this.boundingBox=new F);const e=this.attributes.instanceStart,t=this.attributes.instanceEnd;e!==void 0&&t!==void 0&&(this.boundingBox.setFromBufferAttribute(e),Se.setFromBufferAttribute(t),this.boundingBox.union(Se))}computeBoundingSphere(){this.boundingSphere===null&&(this.boundingSphere=new Ae),this.boundingBox===null&&this.computeBoundingBox();const e=this.attributes.instanceStart,t=this.attributes.instanceEnd;if(e!==void 0&&t!==void 0){const n=this.boundingSphere.center;this.boundingBox.getCenter(n);let i=0;for(let s=0,a=e.count;s<a;s++)q.fromBufferAttribute(e,s),i=Math.max(i,n.distanceToSquared(q)),q.fromBufferAttribute(t,s),i=Math.max(i,n.distanceToSquared(q));this.boundingSphere.radius=Math.sqrt(i),isNaN(this.boundingSphere.radius)&&console.error("THREE.LineSegmentsGeometry.computeBoundingSphere(): Computed radius is NaN. The instanced position data is likely to have NaN values.",this)}}toJSON(){}}Y.line={worldUnits:{value:1},linewidth:{value:1},resolution:{value:new ce},dashOffset:{value:0},dashScale:{value:1},dashSize:{value:1},gapSize:{value:1}};K.line={uniforms:Le.merge([Y.common,Y.fog,Y.line]),vertexShader:`
		#include <common>
		#include <color_pars_vertex>
		#include <fog_pars_vertex>
		#include <logdepthbuf_pars_vertex>
		#include <clipping_planes_pars_vertex>

		uniform float linewidth;
		uniform vec2 resolution;

		attribute vec3 instanceStart;
		attribute vec3 instanceEnd;

		attribute vec3 instanceColorStart;
		attribute vec3 instanceColorEnd;

		#ifdef WORLD_UNITS

			varying vec4 worldPos;
			varying vec3 worldStart;
			varying vec3 worldEnd;

			#ifdef USE_DASH

				varying vec2 vUv;

			#endif

		#else

			varying vec2 vUv;

		#endif

		#ifdef USE_DASH

			uniform float dashScale;
			attribute float instanceDistanceStart;
			attribute float instanceDistanceEnd;
			varying float vLineDistance;

		#endif

		float trimSegmentAlpha( const in vec4 start, const in vec4 end ) {

			// compute the interpolation factor needed to trim the segment so it terminates
			// between the camera plane and the near plane

			// conservative estimate of the near plane
			float a = projectionMatrix[ 2 ][ 2 ]; // 3nd entry in 3th column
			float b = projectionMatrix[ 3 ][ 2 ]; // 3nd entry in 4th column

			// we need different nearEstimate formula for reversed and default depth buffer
			// a is positive with a reversed depth buffer so it can be used for controlling the code flow
			float nearEstimate = ( a > 0.0 ) ? ( - b / ( a + 1.0 ) ) : ( - 0.5 * b / a );

			return ( nearEstimate - start.z ) / ( end.z - start.z );

		}

		void main() {

			#ifdef USE_COLOR

				vColor.xyz = ( position.y < 0.5 ) ? instanceColorStart : instanceColorEnd;

			#endif

			float aspect = resolution.x / resolution.y;

			// camera space
			vec4 start = modelViewMatrix * vec4( instanceStart, 1.0 );
			vec4 end = modelViewMatrix * vec4( instanceEnd, 1.0 );

			#ifdef USE_DASH

				float lineDistanceStart = dashScale * instanceDistanceStart;
				float lineDistanceEnd = dashScale * instanceDistanceEnd;

			#endif

			#ifdef WORLD_UNITS

				worldStart = start.xyz;
				worldEnd = end.xyz;

			#else

				vUv = uv;

			#endif

			// special case for perspective projection, and segments that terminate either in, or behind, the camera plane
			// clearly the gpu firmware has a way of addressing this issue when projecting into ndc space
			// but we need to perform ndc-space calculations in the shader, so we must address this issue directly
			// perhaps there is a more elegant solution -- WestLangley

			bool perspective = ( projectionMatrix[ 2 ][ 3 ] == - 1.0 ); // 4th entry in the 3rd column

			if ( perspective ) {

				if ( start.z < 0.0 && end.z >= 0.0 ) {

					float alpha = trimSegmentAlpha( start, end );
					end.xyz = mix( start.xyz, end.xyz, alpha );

					#ifdef USE_DASH

						lineDistanceEnd = mix( lineDistanceStart, lineDistanceEnd, alpha );

					#endif

				} else if ( end.z < 0.0 && start.z >= 0.0 ) {

					float alpha = trimSegmentAlpha( end, start );
					start.xyz = mix( end.xyz, start.xyz, alpha );

					#ifdef USE_DASH

						lineDistanceStart = mix( lineDistanceEnd, lineDistanceStart, alpha );

					#endif

				}

			}

			#ifdef USE_DASH

				vLineDistance = ( position.y < 0.5 ) ? lineDistanceStart : lineDistanceEnd;
				vUv = uv;

			#endif

			// clip space
			vec4 clipStart = projectionMatrix * start;
			vec4 clipEnd = projectionMatrix * end;

			// ndc space
			vec3 ndcStart = clipStart.xyz / clipStart.w;
			vec3 ndcEnd = clipEnd.xyz / clipEnd.w;

			// direction
			vec2 dir = ndcEnd.xy - ndcStart.xy;

			// account for clip-space aspect ratio
			dir.x *= aspect;
			dir = normalize( dir );

			#ifdef WORLD_UNITS

				vec3 worldDir = normalize( end.xyz - start.xyz );
				vec3 tmpFwd = normalize( mix( start.xyz, end.xyz, 0.5 ) );
				vec3 worldUp = normalize( cross( worldDir, tmpFwd ) );
				vec3 worldFwd = cross( worldDir, worldUp );
				worldPos = position.y < 0.5 ? start: end;

				// height offset
				float hw = linewidth * 0.5;
				worldPos.xyz += position.x < 0.0 ? hw * worldUp : - hw * worldUp;

				// don't extend the line if we're rendering dashes because we
				// won't be rendering the endcaps
				#ifndef USE_DASH

					// cap extension
					worldPos.xyz += position.y < 0.5 ? - hw * worldDir : hw * worldDir;

					// add width to the box
					worldPos.xyz += worldFwd * hw;

					// endcaps
					if ( position.y > 1.0 || position.y < 0.0 ) {

						worldPos.xyz -= worldFwd * 2.0 * hw;

					}

				#endif

				// project the worldpos
				vec4 clip = projectionMatrix * worldPos;

				// shift the depth of the projected points so the line
				// segments overlap neatly
				vec3 clipPose = ( position.y < 0.5 ) ? ndcStart : ndcEnd;
				clip.z = clipPose.z * clip.w;

			#else

				vec2 offset = vec2( dir.y, - dir.x );
				// undo aspect ratio adjustment
				dir.x /= aspect;
				offset.x /= aspect;

				// sign flip
				if ( position.x < 0.0 ) offset *= - 1.0;

				// endcaps
				if ( position.y < 0.0 ) {

					offset += - dir;

				} else if ( position.y > 1.0 ) {

					offset += dir;

				}

				// adjust for linewidth
				offset *= linewidth;

				// adjust for clip-space to screen-space conversion // maybe resolution should be based on viewport ...
				offset /= resolution.y;

				// select end
				vec4 clip = ( position.y < 0.5 ) ? clipStart : clipEnd;

				// back to clip space
				offset *= clip.w;

				clip.xy += offset;

			#endif

			gl_Position = clip;

			vec4 mvPosition = ( position.y < 0.5 ) ? start : end; // this is an approximation

			#include <logdepthbuf_vertex>
			#include <clipping_planes_vertex>
			#include <fog_vertex>

		}
		`,fragmentShader:`
		uniform vec3 diffuse;
		uniform float opacity;
		uniform float linewidth;

		#ifdef USE_DASH

			uniform float dashOffset;
			uniform float dashSize;
			uniform float gapSize;

		#endif

		varying float vLineDistance;

		#ifdef WORLD_UNITS

			varying vec4 worldPos;
			varying vec3 worldStart;
			varying vec3 worldEnd;

			#ifdef USE_DASH

				varying vec2 vUv;

			#endif

		#else

			varying vec2 vUv;

		#endif

		#include <common>
		#include <color_pars_fragment>
		#include <fog_pars_fragment>
		#include <logdepthbuf_pars_fragment>
		#include <clipping_planes_pars_fragment>

		vec2 closestLineToLine(vec3 p1, vec3 p2, vec3 p3, vec3 p4) {

			float mua;
			float mub;

			vec3 p13 = p1 - p3;
			vec3 p43 = p4 - p3;

			vec3 p21 = p2 - p1;

			float d1343 = dot( p13, p43 );
			float d4321 = dot( p43, p21 );
			float d1321 = dot( p13, p21 );
			float d4343 = dot( p43, p43 );
			float d2121 = dot( p21, p21 );

			float denom = d2121 * d4343 - d4321 * d4321;

			float numer = d1343 * d4321 - d1321 * d4343;

			mua = numer / denom;
			mua = clamp( mua, 0.0, 1.0 );
			mub = ( d1343 + d4321 * ( mua ) ) / d4343;
			mub = clamp( mub, 0.0, 1.0 );

			return vec2( mua, mub );

		}

		void main() {

			float alpha = opacity;
			vec4 diffuseColor = vec4( diffuse, alpha );

			#include <clipping_planes_fragment>

			#ifdef USE_DASH

				if ( vUv.y < - 1.0 || vUv.y > 1.0 ) discard; // discard endcaps

				if ( mod( vLineDistance + dashOffset, dashSize + gapSize ) > dashSize ) discard; // todo - FIX

			#endif

			#ifdef WORLD_UNITS

				// Find the closest points on the view ray and the line segment
				vec3 rayEnd = normalize( worldPos.xyz ) * 1e5;
				vec3 lineDir = worldEnd - worldStart;
				vec2 params = closestLineToLine( worldStart, worldEnd, vec3( 0.0, 0.0, 0.0 ), rayEnd );

				vec3 p1 = worldStart + lineDir * params.x;
				vec3 p2 = rayEnd * params.y;
				vec3 delta = p1 - p2;
				float len = length( delta );
				float norm = len / linewidth;

				#ifndef USE_DASH

					#ifdef USE_ALPHA_TO_COVERAGE

						float dnorm = fwidth( norm );
						alpha = 1.0 - smoothstep( 0.5 - dnorm, 0.5 + dnorm, norm );

					#else

						if ( norm > 0.5 ) {

							discard;

						}

					#endif

				#endif

			#else

				#ifdef USE_ALPHA_TO_COVERAGE

					// artifacts appear on some hardware if a derivative is taken within a conditional
					float a = vUv.x;
					float b = ( vUv.y > 0.0 ) ? vUv.y - 1.0 : vUv.y + 1.0;
					float len2 = a * a + b * b;
					float dlen = fwidth( len2 );

					if ( abs( vUv.y ) > 1.0 ) {

						alpha = 1.0 - smoothstep( 1.0 - dlen, 1.0 + dlen, len2 );

					}

				#else

					if ( abs( vUv.y ) > 1.0 ) {

						float a = vUv.x;
						float b = ( vUv.y > 0.0 ) ? vUv.y - 1.0 : vUv.y + 1.0;
						float len2 = a * a + b * b;

						if ( len2 > 1.0 ) discard;

					}

				#endif

			#endif

			#include <logdepthbuf_fragment>
			#include <color_fragment>

			gl_FragColor = vec4( diffuseColor.rgb, alpha );

			#include <tonemapping_fragment>
			#include <colorspace_fragment>
			#include <fog_fragment>
			#include <premultiplied_alpha_fragment>

		}
		`};class De extends Ie{constructor(e){super({type:"LineMaterial",uniforms:Le.clone(K.line.uniforms),vertexShader:K.line.vertexShader,fragmentShader:K.line.fragmentShader,clipping:!0}),this.isLineMaterial=!0,this.setValues(e)}get color(){return this.uniforms.diffuse.value}set color(e){this.uniforms.diffuse.value=e}get worldUnits(){return"WORLD_UNITS"in this.defines}set worldUnits(e){e===!0!==this.worldUnits&&(this.needsUpdate=!0),e===!0?this.defines.WORLD_UNITS="":delete this.defines.WORLD_UNITS}get linewidth(){return this.uniforms.linewidth.value}set linewidth(e){this.uniforms.linewidth&&(this.uniforms.linewidth.value=e)}get dashed(){return"USE_DASH"in this.defines}set dashed(e){e===!0!==this.dashed&&(this.needsUpdate=!0),e===!0?this.defines.USE_DASH="":delete this.defines.USE_DASH}get dashScale(){return this.uniforms.dashScale.value}set dashScale(e){this.uniforms.dashScale.value=e}get dashSize(){return this.uniforms.dashSize.value}set dashSize(e){this.uniforms.dashSize.value=e}get dashOffset(){return this.uniforms.dashOffset.value}set dashOffset(e){this.uniforms.dashOffset.value=e}get gapSize(){return this.uniforms.gapSize.value}set gapSize(e){this.uniforms.gapSize.value=e}get opacity(){return this.uniforms.opacity.value}set opacity(e){this.uniforms&&(this.uniforms.opacity.value=e)}get resolution(){return this.uniforms.resolution.value}set resolution(e){this.uniforms.resolution.value.copy(e)}get alphaToCoverage(){return"USE_ALPHA_TO_COVERAGE"in this.defines}set alphaToCoverage(e){this.defines&&(e===!0!==this.alphaToCoverage&&(this.needsUpdate=!0),e===!0?this.defines.USE_ALPHA_TO_COVERAGE="":delete this.defines.USE_ALPHA_TO_COVERAGE)}}const ie=new $,Me=new p,Ee=new p,b=new $,x=new $,A=new $,se=new p,oe=new le,S=new Te,ze=new p,X=new F,Q=new Ae,L=new $;let U,I;function _e(f,e,t){return L.set(0,0,-e,1).applyMatrix4(f.projectionMatrix),L.multiplyScalar(1/L.w),L.x=I/t.width,L.y=I/t.height,L.applyMatrix4(f.projectionMatrixInverse),L.multiplyScalar(1/L.w),Math.abs(Math.max(L.x,L.y))}function Ye(f,e){const t=f.matrixWorld,n=f.geometry,i=n.attributes.instanceStart,s=n.attributes.instanceEnd,a=Math.min(n.instanceCount,i.count);for(let o=0,r=a;o<r;o++){S.start.fromBufferAttribute(i,o),S.end.fromBufferAttribute(s,o),S.applyMatrix4(t);const l=new p,c=new p;U.distanceSqToSegment(S.start,S.end,c,l),c.distanceTo(l)<I*.5&&e.push({point:c,pointOnLine:l,distance:U.origin.distanceTo(c),object:f,face:null,faceIndex:o,uv:null,uv1:null})}}function Ze(f,e,t){const n=e.projectionMatrix,s=f.material.resolution,a=f.matrixWorld,o=f.geometry,r=o.attributes.instanceStart,l=o.attributes.instanceEnd,c=Math.min(o.instanceCount,r.count),d=-e.near;U.at(1,A),A.w=1,A.applyMatrix4(e.matrixWorldInverse),A.applyMatrix4(n),A.multiplyScalar(1/A.w),A.x*=s.x/2,A.y*=s.y/2,A.z=0,se.copy(A),oe.multiplyMatrices(e.matrixWorldInverse,a);for(let h=0,g=c;h<g;h++){if(b.fromBufferAttribute(r,h),x.fromBufferAttribute(l,h),b.w=1,x.w=1,b.applyMatrix4(oe),x.applyMatrix4(oe),b.z>d&&x.z>d)continue;if(b.z>d){const z=b.z-x.z,_=(b.z-d)/z;b.lerp(x,_)}else if(x.z>d){const z=x.z-b.z,_=(x.z-d)/z;x.lerp(b,_)}b.applyMatrix4(n),x.applyMatrix4(n),b.multiplyScalar(1/b.w),x.multiplyScalar(1/x.w),b.x*=s.x/2,b.y*=s.y/2,x.x*=s.x/2,x.y*=s.y/2,S.start.copy(b),S.start.z=0,S.end.copy(x),S.end.z=0;const M=S.closestPointToPointParameter(se,!0);S.at(M,ze);const E=ke.lerp(b.z,x.z,M),H=E>=-1&&E<=1,B=se.distanceTo(ze)<I*.5;if(H&&B){S.start.fromBufferAttribute(r,h),S.end.fromBufferAttribute(l,h),S.start.applyMatrix4(a),S.end.applyMatrix4(a);const z=new p,_=new p;U.distanceSqToSegment(S.start,S.end,_,z),t.push({point:_,pointOnLine:z,distance:U.origin.distanceTo(_),object:f,face:null,faceIndex:h,uv:null,uv1:null})}}}class et extends Z{constructor(e=new Ue,t=new De({color:Math.random()*16777215})){super(e,t),this.isLineSegments2=!0,this.type="LineSegments2"}computeLineDistances(){const e=this.geometry,t=e.attributes.instanceStart,n=e.attributes.instanceEnd,i=new Float32Array(2*t.count);for(let a=0,o=0,r=t.count;a<r;a++,o+=2)Me.fromBufferAttribute(t,a),Ee.fromBufferAttribute(n,a),i[o]=o===0?0:i[o-1],i[o+1]=i[o]+Me.distanceTo(Ee);const s=new re(i,2,1);return e.setAttribute("instanceDistanceStart",new R(s,1,0)),e.setAttribute("instanceDistanceEnd",new R(s,1,1)),this}raycast(e,t){const n=this.material.worldUnits,i=e.camera;if(i===null&&!n&&console.error('LineSegments2: "Raycaster.camera" needs to be set in order to raycast against LineSegments2 while worldUnits is set to false.'),n===!1&&(this.material.resolution.x===0||this.material.resolution.y===0))return;const s=e.params.Line2!==void 0&&e.params.Line2.threshold||0;U=e.ray;const a=this.matrixWorld,o=this.geometry,r=this.material;I=r.linewidth+s,o.boundingSphere===null&&o.computeBoundingSphere(),Q.copy(o.boundingSphere).applyMatrix4(a);let l;if(n)l=I*.5;else{const d=Math.max(i.near,Q.distanceToPoint(U.origin));l=_e(i,d,r.resolution)}if(Q.radius+=l,U.intersectsSphere(Q)===!1)return;o.boundingBox===null&&o.computeBoundingBox(),X.copy(o.boundingBox).applyMatrix4(a);let c;if(n)c=I*.5;else{const d=Math.max(i.near,X.distanceToPoint(U.origin));c=_e(i,d,r.resolution)}X.expandByScalar(c),U.intersectsBox(X)!==!1&&(n?Ye(this,t):Ze(this,i,t))}onBeforeRender(e){const t=this.material.uniforms;t&&t.resolution&&(e.getViewport(ie),this.material.uniforms.resolution.value.set(ie.z,ie.w))}}const J=[2200,7200];function tt(f,e,t=1.45){const n=Math.max(f,1),i=e*Math.PI/180,s=n/Math.sin(i/2)*t;return{distance:s,near:Math.max(.1,n/500),far:Math.max(9e4,s+n*4),fogNear:s+n*1.1,fogFar:s+n*5.5}}function nt(f){return f.length===0?null:f.find(t=>t.kind!=="volume")??f[0]??null}const it=6781068,st=240,ot=2.5;class ut{constructor(e){u(this,"scene",new Re);u(this,"camera");u(this,"renderer");u(this,"controls");u(this,"world",new ee);u(this,"fog",new Fe(395794,J[0],J[1]));u(this,"raycaster",new He);u(this,"pointer",new ce);u(this,"pickables",[]);u(this,"disposables",[]);u(this,"frame",0);u(this,"mounted",!0);u(this,"reducedMotion",!1);u(this,"onHover",()=>{});u(this,"onPick",()=>{});u(this,"onExplodeRequest",()=>{});u(this,"onMemberPick",()=>{});u(this,"onMemberOpenRequest",()=>{});u(this,"onPress",()=>{});u(this,"currentLayer",null);u(this,"incidence",new Map);u(this,"heights",new Map);u(this,"coreObjects",[]);u(this,"edgeList",[]);u(this,"byId",new Map);u(this,"solids",new Map);u(this,"outline",null);u(this,"drill",null);u(this,"memberOutline",null);u(this,"levelLines",null);u(this,"size",new ce(1,1));u(this,"remembered",new Map);u(this,"flyTimer",0);u(this,"loop",()=>{this.mounted&&(this.frame=requestAnimationFrame(this.loop),this.controls.update(),this.renderer.render(this.scene,this.camera))});var i;this.reducedMotion=((i=globalThis.matchMedia)==null?void 0:i.call(globalThis,"(prefers-reduced-motion: reduce)").matches)??!1,this.scene.background=new P(395794),this.scene.fog=this.fog,this.camera=new je(44,1,1,9e4),this.camera.position.set(760,620,900),this.renderer=new We({antialias:!0}),this.renderer.setPixelRatio(Math.min(globalThis.devicePixelRatio??1,2)),e.appendChild(this.renderer.domElement),this.controls=new Ne(this.camera,this.renderer.domElement),Object.assign(this.controls,{enableDamping:!0,dampingFactor:.08,minDistance:120,maxDistance:12e3,autoRotate:!0,autoRotateSpeed:.28,maxPolarAngle:Math.PI*.495}),this.scene.add(new $e(10338278,1186342,.95));const t=new pe(16777215,1.1);t.position.set(600,900,500),this.scene.add(t);const n=new pe(9417983,.55);n.position.set(-700,400,-500),this.scene.add(n),this.scene.add(this.world),this.resize(e),this.loop()}onHoverChange(e){this.onHover=e}onPickChange(e){this.onPick=e}onExplode(e){this.onExplodeRequest=e}onMemberPickChange(e){this.onMemberPick=e}onMemberOpen(e){this.onMemberOpenRequest=e}onPressChange(e){this.onPress=e}memberAt(){if(!this.drill)return null;const e=this.raycaster.intersectObject(this.drill.instanced,!1)[0];if(!e||e.instanceId===void 0)return null;const t=this.drill.members[e.instanceId];return t?{instance:e.instanceId,entry:t}:null}resize(e){var i;const t=Math.max(1,e.clientWidth),n=Math.max(1,e.clientHeight);this.camera.aspect=t/n,this.camera.updateProjectionMatrix(),this.renderer.setSize(t,n),this.renderer.getDrawingBufferSize(this.size),(i=this.levelLines)==null||i.resolution.set(this.size.x,this.size.y)}update(e){const t=this.currentLayer===null;this.currentLayer=e.layer,this.clear(),this.pickables.length=0,e.layer==="packages"?this.drawPackages(e):this.drawCommunities(e),t&&this.frameWorld()}drawPackages(e){const{view:t,height:n,footprint:i}=e,s=c=>20/2.4*Math.sqrt(c),a=Xe(t.volumes,t.standalone.length,{seatRadius:s,pad:16}),o=new Map;for(const c of t.volumes){const d=a.volumes.find(E=>E.key===c.key);if(!d)continue;const h=Math.max(...c.packages.map(E=>n(E.size)))+26,g=new j(d.radius*2+40,h,d.radius*2+40),y=new Z(g,new Ge({color:9425151,transparent:!0,opacity:.05,depthWrite:!1}));y.position.set(d.centre[0],h/2-4,d.centre[1]),this.add(y),this.coreObjects.push(y);const M=new W(new te(g),new N({color:11066623,transparent:!0,opacity:.5}));M.position.copy(y.position),this.add(M),this.pickables.push({mesh:y,entry:{kind:"volume",title:c.key,lines:[`namespace group · ${c.packages.length} packages`,`${c.members.toLocaleString()} members · ${c.functions.toLocaleString()} functions`,"derived from the package labels — not an exported object"]}}),c.packages.forEach((E,H)=>{o.set(E.id,Qe(d.centre,s(c.packages.length),H,c.packages.length))})}t.standalone.forEach((c,d,h)=>{const g=d/Math.max(1,h.length)*Math.PI*2;o.set(c.id,[Math.cos(g)*a.artRadius,0,Math.sin(g)*a.artRadius])});const r=[...t.remainder?[t.remainder]:[],...t.artifacts];r.forEach((c,d)=>{const h=Math.PI*.5+(d-r.length/2)*.06;o.set(c.id,[Math.cos(h)*a.artifactRadius,0,Math.sin(h)*a.artifactRadius])});const l=[...t.packages,...r];for(const c of l){const d=o.get(c.id);d&&this.solid(c,d,n(c.size),i(c.functions))}e.edges&&this.web(l,o,t)}drawCommunities(e){const{view:t}=e,n=t.communities,i=Je(n.map(o=>o.members)),s=Ke(n.map(o=>o.functions)),a=new Map;if(70*Math.sqrt(Math.max(1,n.length)),n.forEach((o,r)=>{const l=60*Math.sqrt(r+.6),c=r*2.39996,d=[Math.cos(c)*l,0,Math.sin(c)*l];a.set(o.id,d);const h=s(o.functions),g=new j(h,i(o.members),h),y=new Z(g,new ne({color:new P().setHSL(be(xe(o.id))/360,o.shared?.72:.08,o.shared?.58:.42),roughness:.55,metalness:.12}));y.position.set(d[0],i(o.members)/2,d[2]),this.add(y),this.coreObjects.push(y),this.pickables.push({mesh:y,entry:this.communityEntry(o)})}),e.edges){const o=new Map(n.map(l=>[l.id,Math.min(i(l.members)*.5,46)])),r=this.communityPairs(t);this.webSegments(r.flatMap(([l,c,d])=>{const h=a.get(l),g=a.get(c);return!h||!g?[]:[[[h[0],o.get(l)??6,h[2]],[g[0],o.get(c)??6,g[2]],d]]}))}}communityPairs(e){const t=new Map(e.packages.map(i=>[i.id,i])),n=new Map;for(const i of e.edgeList){const s=t.get(i.source),a=t.get(i.target);if(!s||!a||s.communityId===a.communityId)continue;const o=s.communityId<a.communityId?`${s.communityId}|${a.communityId}`:`${a.communityId}|${s.communityId}`;n.set(o,(n.get(o)??0)+i.weight)}return[...n.entries()].map(([i,s])=>{const[a,o]=i.split("|").map(Number);return[a,o,s]})}solid(e,t,n,i){const s=e.kind!=="package",a=s?new P(7036794):e.sharesCommunity?new P().setHSL(be(xe(e.communityId))/360,.72,.58):new P(it),o=new j(i,n,i),r=new Z(o,new ne({color:a,roughness:.55,metalness:.12}));if(r.position.set(t[0],n/2,t[2]),this.add(r),this.heights.set(e.id,n),this.solids.set(e.id,{pos:[t[0],0,t[2]],side:i,height:n,mesh:r}),e.kind==="package"&&this.coreObjects.push(r),s){const l=new W(new te(o),new N({color:16764794,transparent:!0,opacity:.85}));l.position.copy(r.position),this.add(l)}this.pickables.push({mesh:r,entry:{kind:s?"bucket":"package",id:e.id,title:e.label,lines:[s?"not a package — a producer bucket":`package · ${e.size.toLocaleString()} members · ${e.functions.toLocaleString()} functions`,s?`${e.size.toLocaleString()} members · ${e.functions.toLocaleString()} functions`:e.communityLabel&&e.communityLabel!==String(e.communityId)?`namespace ${e.namespace} · community ${e.communityId} (${e.communityLabel})`:`namespace ${e.namespace} · community ${e.communityId}`,s?"its coupling is aggregated over unrelated files, so it is not drawn":e.sharesCommunity?"shares its community":"alone in its community"]}})}communityEntry(e){return{kind:"community",title:e.label,lines:[`community · ${e.members.toLocaleString()} members over ${e.packageCount} node${e.packageCount===1?"":"s"}`,e.shared?"holds more than one node":"holds exactly one node",e.containsBucket?"includes a producer bucket, not only packages":"packages only"]}}web(e,t,n){this.incidence.clear();const i=[],s=this.edgeList;for(const a of s){const o=t.get(a.source),r=t.get(a.target);if(!o||!r)continue;const l=this.byId.get(a.source),c=this.byId.get(a.target);if(!l||!c)continue;const d=this.incidence.get(a.source)??[];d.push({id:a.target,weight:a.weight}),this.incidence.set(a.source,d);const h=Math.min((this.heights.get(a.source)??10)*.5,46),g=Math.min((this.heights.get(a.target)??10)*.5,46);i.push([[o[0],h,o[2]],[r[0],g,r[2]],a.weight])}this.webSegments(i)}webSegments(e){if(e.length===0)return;const t=e.map(d=>d[2]).sort((d,h)=>d-h),n=Math.log(t[0]||1),i=Math.log(t[t.length-1]||1),s=Math.max(1e-6,i-n),a=new Float32Array(e.length*6),o=new Float32Array(e.length*6),r=new P;e.forEach(([d,h,g],y)=>{const M=(Math.log(g)-n)/s;a.set([d[0],d[1],d[2],h[0],h[1],h[2]],y*6),r.setHSL(.09+.03*M,.85,.26+.26*M),o.set([r.r,r.g,r.b,r.r,r.g,r.b],y*6)});const l=new ge;l.setAttribute("position",new ye(a,3)),l.setAttribute("color",new ye(o,3));const c=new N({vertexColors:!0,transparent:!0,opacity:.5,blending:we,depthWrite:!1});this.add(new W(l,c))}setEdges(e,t){this.edgeList=e,this.byId.clear();for(const[n,i]of t)this.byId.set(n,i)}expand(e,t,n,i={}){this.collapse();const s=this.solids.get(e);if(!s||t.length===0)return{drawn:0,edges:0,measuredHeight:!1,boundary:0};const a=this.controls.autoRotate,o=this.controls.autoRotateSpeed;this.controls.autoRotate=!1;const r={position:this.camera.position.clone(),target:this.controls.target.clone(),near:this.camera.near,far:this.camera.far,maxDistance:this.controls.maxDistance,fogNear:this.fog.near,fogFar:this.fog.far,autoRotate:a,autoRotateSpeed:o};this.world.visible=!1;const l=new ee,c=Math.max(0,...t.map(m=>m.complexity||0)),d=c>0,h=t.length,g=Math.max(60,26*Math.sqrt(h)),y=g*2/Math.sqrt(h),M=d?y*2.4:y*1.1,E=new j(1,1,1),H=new ne({color:i.color??new P(9425151),roughness:.5,metalness:.15}),B=new Ve(E,H,h);B.instanceMatrix.setUsage(qe);const z=new Map,_=new le,de=new p;t.forEach((m,w)=>{const v=g*Math.sqrt((w+.5)/h),D=w*2.39996,T=Math.cos(D)*v,k=Math.sin(D)*v,O=d?Math.max(y*.15,m.complexity/c*(i.maxHeight??M)):M;de.set(y*.55,O,y*.55),_.compose(new p(T,O/2,k),new ve,de),B.setMatrixAt(w,_),z.set(m.index,new p(T,O,k))}),B.instanceMatrix.needsUpdate=!0,l.add(B);const G=[];for(const m of n){const w=z.get(m.source),v=z.get(m.target);!w||!v||G.push(w.x,w.y*.6,w.z,v.x,v.y*.6,v.z)}if(G.length>0){const m=new ge;m.setAttribute("position",new ae(G,3)),l.add(new W(m,new N({color:10479816,transparent:!0,opacity:.55})))}const V=[];for(const m of this.edgeList){const w=m.source===e?m.target:m.target===e?m.source:null;if(w===null)continue;const v=this.solids.get(w);if(!v)continue;const D=new p(v.pos[0]-s.pos[0],0,v.pos[2]-s.pos[2]);D.lengthSq()<1e-6||V.push({direction:D.normalize(),weight:m.weight})}if(V.length>0){const m=[],w=[],v=new P(16767130),D=[1,1.6,2.4],T=[1,.55,.3];for(const{direction:ue}of V)for(let C=0;C<D.length-1;C+=1){const fe=ue.clone().multiplyScalar(g*D[C]),me=ue.clone().multiplyScalar(g*D[C+1]);m.push(fe.x,2,fe.z,me.x,2,me.z),w.push(v.r,v.g,v.b,v.r*T[C+1],v.g*T[C+1],v.b*T[C+1])}const k=new Ue;k.setPositions(m),k.setColors(w);const O=new De({color:16777215,vertexColors:!0,linewidth:ot,transparent:!0,opacity:.55,depthWrite:!1,worldUnits:!1});O.resolution.set(this.size.x,this.size.y),this.levelLines=O,l.add(new et(k,O))}this.scene.add(l);const he=new Map;return t.forEach((m,w)=>he.set(m.index,w)),this.drill={group:l,subjectId:e,cameraBefore:r,instanced:B,slots:he,members:t.map((m,w)=>({index:m.index,name:m.name??`node ${m.index}`,nodeTypeName:m.nodeTypeName??"node",filePath:m.filePath??null,instance:w}))},this.frameSphere(new p(0,0,0),Math.hypot(g+y,M),1.45),{drawn:h,edges:G.length/6,measuredHeight:d,boundary:V.length}}collapse(){if(!this.drill)return;const{group:e,cameraBefore:t}=this.drill;this.scene.remove(e),e.traverse(n=>{var a;const i=n;(a=i.geometry)==null||a.dispose();const s=i.material;Array.isArray(s)?s.forEach(o=>o.dispose()):s==null||s.dispose()}),this.world.visible=!0,this.camera.position.copy(t.position),this.controls.target.copy(t.target),this.camera.near=t.near,this.camera.far=t.far,this.controls.maxDistance=t.maxDistance,this.fog.near=t.fogNear,this.fog.far=t.fogFar,this.camera.updateProjectionMatrix(),this.controls.update(),this.levelLines=null,this.controls.autoRotate=t.autoRotate,this.controls.autoRotateSpeed=t.autoRotateSpeed,this.drill=null}get isExpanded(){return this.drill!==null}highlightSolid(e){this.disposeOutline();for(const a of this.coreObjects){const o=a.material;if(!o||Array.isArray(o))continue;const r=o,l=this.remember(r);r.opacity=e===null?l.opacity:l.opacity*.18,r.transparent=l.transparent||e!==null,r.emissive&&(r.emissive.setHex(l.emissive),r.emissiveIntensity=l.emissiveIntensity)}if(e===null||this.drill)return;const t=this.boxFor(e);if(!t)return;const i=this.objectFor(e).material;if(i){const a=this.remember(i);i.opacity=a.opacity,i.transparent=a.transparent,i.emissive&&(i.emissive.setHex(3057919),i.emissiveIntensity=.85)}const s=this.glowHalo(t,9425151);this.world.add(s),this.outline=s}glowHalo(e,t){const n=new ee,i=e.getSize(new p),s=e.getCenter(new p),a=Math.max(3,Math.max(i.x,i.z)*.045);for(const[o,r]of[[0,1],[1,.45],[2.2,.18]]){const l=new te(new j(i.x+a*o,i.y+a*o,i.z+a*o)),c=new W(l,new N({color:t,transparent:!0,opacity:r,blending:we,depthTest:!1,depthWrite:!1}));c.position.copy(s),c.renderOrder=2,n.add(c)}return n}disposeOutline(){var e;this.outline&&((e=this.outline.parent)==null||e.remove(this.outline),this.outline.traverse(t=>{var s;const n=t;(s=n.geometry)==null||s.dispose();const i=n.material;i==null||i.dispose()}),this.outline=null)}highlightMember(e){var r;if(this.memberOutline&&((r=this.memberOutline.parent)==null||r.remove(this.memberOutline),this.memberOutline.traverse(l=>{var d,h;const c=l;(d=c.geometry)==null||d.dispose(),(h=c.material)==null||h.dispose()}),this.memberOutline=null),!e||!this.drill)return;const t=e.instance??this.drill.slots.get(e.index);if(t===void 0)return;const n=new le;this.drill.instanced.getMatrixAt(t,n);const i=new p,s=new p;n.decompose(i,new ve,s);const a=new F().setFromCenterAndSize(i,s),o=this.glowHalo(a,16764794);this.drill.group.add(o),this.memberOutline=o}remember(e){var s;const t=this.remembered.get(e);if(t)return t;const n=e,i={opacity:e.opacity,transparent:e.transparent,emissive:((s=n.emissive)==null?void 0:s.getHex())??0,emissiveIntensity:n.emissiveIntensity??1};return this.remembered.set(e,i),i}objectFor(e){var t,n;return((t=this.solids.get(e))==null?void 0:t.mesh)??((n=this.pickables.find(i=>i.entry.id===e))==null?void 0:n.mesh)??null}boxFor(e){const t=this.objectFor(e);if(!t)return null;const n=new F().setFromObject(t);return n.isEmpty()?null:n}focusSolid(e){this.cancelPendingFocus(),this.highlightSolid(e),this.flyToSolid(e)}focusSoon(e){this.cancelPendingFocus(),this.highlightSolid(e),e!==null&&(this.flyTimer=globalThis.setTimeout(()=>{this.flyTimer=0,this.flyToSolid(e)},st))}cancelPendingFocus(){this.flyTimer&&(globalThis.clearTimeout(this.flyTimer),this.flyTimer=0)}flyToSolid(e){if(e===null)return;const t=this.boxFor(e);if(!t)return;const n=t.getSize(new p),i=t.getCenter(new p),s=Math.max(n.x,n.z);this.flyTo(i,Math.min(1400,Math.max(180,s*3.4)))}setDrift(e){this.controls.autoRotate=e}flyTo(e,t){const n=this.camera,i=new p().subVectors(n.position,this.controls.target).normalize(),s=e.clone().addScaledVector(i,t);if(this.reducedMotion){n.position.copy(s),this.controls.target.copy(e),this.controls.update();return}const a=n.position.clone(),o=this.controls.target.clone(),r=performance.now(),l=()=>{const c=Math.min(1,(performance.now()-r)/420),d=1-Math.pow(1-c,3);n.position.lerpVectors(a,s,d),this.controls.target.lerpVectors(o,e,d),this.controls.update(),c<1&&this.mounted&&requestAnimationFrame(l)};l()}pickAt(){const e=this.raycaster.intersectObjects(this.pickables.map(n=>n.mesh),!1),t=[];for(const n of e){const i=this.pickables.find(s=>s.mesh===n.object);i&&t.push(i.entry)}return nt(t)}add(e){this.world.add(e),e.traverse(t=>{const n=t.geometry;n&&this.disposables.push(n);const i=t.material;Array.isArray(i)?this.disposables.push(...i):i&&this.disposables.push(i)})}clear(){this.collapse(),this.solids.clear();for(const e of[...this.world.children])this.world.remove(e);for(const e of this.disposables)e.dispose();this.disposables.length=0,this.pickables.length=0,this.incidence.clear(),this.heights.clear(),this.coreObjects.length=0}frameSphere(e,t,n=1.45){const i=tt(Math.max(t,1),this.camera.fov,n);this.controls.target.copy(e),this.controls.maxDistance=Math.max(9e3,t*10);const s=new p(.55,.62,.75).normalize();this.camera.position.copy(e).addScaledVector(s,i.distance),this.camera.near=i.near,this.camera.far=i.far,this.camera.updateProjectionMatrix(),this.fog.near=i.fogNear,this.fog.far=i.fogFar,this.controls.update()}frameWorld(){const e=new F;for(const r of this.coreObjects)e.expandByObject(r);if(e.isEmpty()&&e.setFromObject(this.world),e.isEmpty())return;const t=e.getCenter(new p),n=new p(t.x,e.min.y,t.z);let i=0;for(const r of[e.min.x,e.max.x])for(const l of[e.min.y,e.max.y])for(const c of[e.min.z,e.max.z])i=Math.max(i,new p(r,l,c).distanceTo(n));i=Math.max(i,1),this.controls.target.copy(n),this.controls.maxDistance=Math.max(9e3,i*10);const s=this.camera.fov*Math.PI/180,a=i/Math.sin(s/2)*1.25,o=new p(.55,.62,.75).normalize();this.camera.position.copy(n).addScaledVector(o,a),this.camera.near=Math.max(1,i/500),this.camera.far=Math.max(9e4,i*40),this.camera.updateProjectionMatrix(),this.fog.near=J[0],this.fog.far=J[1],this.controls.update()}attachPointer(e){let t=null,n=null;const i=r=>{const l=e.getBoundingClientRect();this.pointer.set((r.clientX-l.left)/l.width*2-1,-((r.clientY-l.top)/l.height)*2+1),this.raycaster.setFromCamera(this.pointer,this.camera);const c=this.memberAt();if(c){this.onHover({kind:"package",title:c.entry.name,lines:[`${c.entry.nodeTypeName}${c.entry.filePath?` · ${c.entry.filePath}`:""}`,"click for its name · double-click to open it in Understand"]});return}if(this.drill){this.onHover(null);return}this.onHover(this.pickAt())},s=()=>this.onHover(null),a=()=>{this.raycaster.setFromCamera(this.pointer,this.camera),this.onPress();const r=this.memberAt();if(r){t=r.entry,n=null,this.onMemberPick(r.entry);return}t=null,n=null,this.onMemberPick(null),!this.drill&&(n=this.pickAt(),this.onPick(n),this.focusSoon(n&&n.id!==void 0?n.id:null))},o=()=>{if(this.cancelPendingFocus(),t){this.onMemberOpenRequest(t);return}n&&this.onExplodeRequest(n)};e.addEventListener("pointermove",i),e.addEventListener("pointerleave",s),e.addEventListener("pointerdown",a),e.addEventListener("dblclick",o)}dispose(){this.mounted=!1,this.cancelPendingFocus(),cancelAnimationFrame(this.frame),this.clear(),this.controls.dispose(),this.renderer.dispose(),this.renderer.domElement.remove()}}function ft(){try{const f=document.createElement("canvas");return!!(f.getContext("webgl2")??f.getContext("webgl")??f.getContext("experimental-webgl"))}catch{return!1}}export{ut as UniverseScene,nt as nearestSolid,ft as webglAvailable};
