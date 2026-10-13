import{b as a}from"./index-B6RlLcne.js";const c=new Map;async function i(t){const r=c.get(t);if(r!==void 0)return r;const e=await fetch(a(`sources/${t}.txt`));if(!e.ok)throw new Error(`sources/${t}.txt: HTTP ${e.status}`);const o=await e.text();return c.set(t,o),o}async function f(t){return t.source?t.source:t.source_id?i(t.source_id):""}function x(t,r,e){if(!r||!e||r<1)return t;const o=t.split(`
`),n=Math.max(0,r-1),s=Math.min(o.length,e);return o.slice(n,s).join(`
`)}export{x as e,i as l,f as r};
