import{p as q}from"./chunk-WASTHULE-BJg-IYUK.js";import{p as H}from"./wardley-RL74JXVD-T2LBEBUU-zrUa84uF.js";import{g as J,s as Y,a as tt,b as et,x as at,v as it,c as s,l as w,d as rt,K as st,aP as ot,aQ as lt,aR as M,aS as nt,f as ct,C as dt,aT as pt,L as gt}from"./Mermaid.vue_vue_type_script_setup_true_lang-CtkE6-tL.js";import"./chunk-MFRUYFWM-mTt0Pu-Z.js";import"./index-DF_rg-rh.js";import"./modules/vue-DYyGvQDd.js";import"./modules/shiki-Ca4pGaxy.js";import"./modules/file-saver-B7oFTzqn.js";var ht=gt.pie,C={sections:new Map,showData:!1},u=C.sections,D=C.showData,ut=structuredClone(ht),ft=s(()=>structuredClone(ut),"getConfig"),mt=s(()=>{u=new Map,D=C.showData,dt()},"clear"),vt=s(({label:t,value:a})=>{if(a<0)throw new Error(`"${t}" has invalid value: ${a}. Negative values are not allowed in pie charts. All slice values must be >= 0.`);u.has(t)||(u.set(t,a),w.debug(`added new section: ${t}, with value: ${a}`))},"addSection"),xt=s(()=>u,"getSections"),St=s(t=>{D=t},"setShowData"),wt=s(()=>D,"getShowData"),G={getConfig:ft,clear:mt,setDiagramTitle:it,getDiagramTitle:at,setAccTitle:et,getAccTitle:tt,setAccDescription:Y,getAccDescription:J,addSection:vt,getSections:xt,setShowData:St,getShowData:wt},Ct=s((t,a)=>{q(t,a),a.setShowData(t.showData),t.sections.map(a.addSection)},"populateDb"),Dt={parse:s(async t=>{const a=await H("pie",t);w.debug(a),Ct(a,G)},"parse")},$t=s(t=>`
  .pieCircle{
    stroke: ${t.pieStrokeColor};
    stroke-width : ${t.pieStrokeWidth};
    opacity : ${t.pieOpacity};
  }
  .pieOuterCircle{
    stroke: ${t.pieOuterStrokeColor};
    stroke-width: ${t.pieOuterStrokeWidth};
    fill: none;
  }
  .pieTitleText {
    text-anchor: middle;
    font-size: ${t.pieTitleTextSize};
    fill: ${t.pieTitleTextColor};
    font-family: ${t.fontFamily};
  }
  .slice {
    font-family: ${t.fontFamily};
    fill: ${t.pieSectionTextColor};
    font-size:${t.pieSectionTextSize};
    // fill: white;
  }
  .legend text {
    fill: ${t.pieLegendTextColor};
    font-family: ${t.fontFamily};
    font-size: ${t.pieLegendTextSize};
  }
`,"getStyles"),yt=$t,Tt=s(t=>{const a=[...t.values()].reduce((r,l)=>r+l,0),$=[...t.entries()].map(([r,l])=>({label:r,value:l})).filter(r=>r.value/a*100>=1);return pt().value(r=>r.value).sort(null)($)},"createPieArcs"),At=s((t,a,$,y)=>{var F;w.debug(`rendering pie chart
`+t);const r=y.db,l=rt(),T=st(r.getConfig(),l.pie),A=40,o=18,p=4,c=450,d=c,f=ot(a),n=f.append("g");n.attr("transform","translate("+d/2+","+c/2+")");const{themeVariables:i}=l;let[_]=lt(i.pieOuterStrokeWidth);_??(_=2);const b=T.textPosition,g=Math.min(d,c)/2-A,P=M().innerRadius(0).outerRadius(g),B=M().innerRadius(g*b).outerRadius(g*b);n.append("circle").attr("cx",0).attr("cy",0).attr("r",g+_/2).attr("class","pieOuterCircle");const h=r.getSections(),O=Tt(h),I=[i.pie1,i.pie2,i.pie3,i.pie4,i.pie5,i.pie6,i.pie7,i.pie8,i.pie9,i.pie10,i.pie11,i.pie12];let m=0;h.forEach(e=>{m+=e});const E=O.filter(e=>(e.data.value/m*100).toFixed(0)!=="0"),v=nt(I).domain([...h.keys()]);n.selectAll("mySlices").data(E).enter().append("path").attr("d",P).attr("fill",e=>v(e.data.label)).attr("class","pieCircle"),n.selectAll("mySlices").data(E).enter().append("text").text(e=>(e.data.value/m*100).toFixed(0)+"%").attr("transform",e=>"translate("+B.centroid(e)+")").style("text-anchor","middle").attr("class","slice");const N=n.append("text").text(r.getDiagramTitle()).attr("x",0).attr("y",-400/2).attr("class","pieTitleText"),k=[...h.entries()].map(([e,S])=>({label:e,value:S})),x=n.selectAll(".legend").data(k).enter().append("g").attr("class","legend").attr("transform",(e,S)=>{const L=o+p,X=L*k.length/2,Z=12*o,j=S*L-X;return"translate("+Z+","+j+")"});x.append("rect").attr("width",o).attr("height",o).style("fill",e=>v(e.label)).style("stroke",e=>v(e.label)),x.append("text").attr("x",o+p).attr("y",o-p).text(e=>r.getShowData()?`${e.label} [${e.value}]`:e.label);const U=Math.max(...x.selectAll("text").nodes().map(e=>(e==null?void 0:e.getBoundingClientRect().width)??0)),K=d+A+o+p+U,R=((F=N.node())==null?void 0:F.getBoundingClientRect().width)??0,Q=d/2-R/2,V=d/2+R/2,W=Math.min(0,Q),z=Math.max(K,V)-W;f.attr("viewBox",`${W} 0 ${z} ${c}`),ct(f,c,z,T.useMaxWidth)},"draw"),_t={draw:At},Gt={parser:Dt,db:G,renderer:_t,styles:yt};export{Gt as diagram};
