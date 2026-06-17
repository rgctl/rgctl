(() => {
  const width = () => document.getElementById('graph').clientWidth;
  const height = () => document.getElementById('graph').clientHeight;

  let svg, g, simulation, linkSel, nodeSel;
  let graphNodes = [];
  let graphLinks = [];
  let expanded = new Set();

  const colors = {
    Function: '#58a6ff', Class: '#bc8cff', Struct: '#79c0ff',
    File: '#8b949e', Module: '#3fb950', default: '#6e7681'
  };

  async function fetchJson(path) {
    const r = await fetch(path);
    if (!r.ok) throw new Error(await r.text());
    return r.json();
  }

  function initSvg() {
    const el = d3.select('#graph');
    el.selectAll('*').remove();
    svg = el.attr('viewBox', [0, 0, width(), height()]);
    g = svg.append('g');
    svg.call(d3.zoom().scaleExtent([0.2, 4]).on('zoom', (e) => g.attr('transform', e.transform)));
  }

  function render() {
    linkSel = g.selectAll('.link').data(graphLinks, d => `${d.from}-${d.to}`);
    linkSel.exit().remove();
    linkSel = linkSel.enter().append('line').attr('class', 'link')
      .attr('stroke', '#30363d').attr('stroke-width', 1.2)
      .merge(linkSel);

    nodeSel = g.selectAll('.node').data(graphNodes, d => d.id);
    nodeSel.exit().remove();
    const enter = nodeSel.enter().append('g').attr('class', 'node').style('cursor', 'grab')
      .call(d3.drag()
        .on('start', (e, d) => { if (!e.active) simulation.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; })
        .on('drag', (e, d) => { d.fx = e.x; d.fy = e.y; })
        .on('end', (e, d) => { if (!e.active) simulation.alphaTarget(0); d.fx = null; d.fy = null; })
      )
      .on('click', (_, d) => showDetail(d))
      .on('dblclick', (_, d) => expandNeighbors(d));

    enter.append('circle').attr('r', 10);
    enter.append('text').attr('x', 12).attr('y', 4).attr('fill', '#e6edf3').attr('font-size', 10);
    nodeSel = enter.merge(nodeSel);
    nodeSel.select('circle').attr('fill', d => colors[d.type] || colors.default);
    nodeSel.select('text').text(d => d.name.length > 18 ? d.name.slice(0, 16) + '…' : d.name);

    simulation = d3.forceSimulation(graphNodes)
      .force('link', d3.forceLink(graphLinks).id(d => d.id).distance(80))
      .force('charge', d3.forceManyBody().strength(-180))
      .force('center', d3.forceCenter(width() / 2, height() / 2))
      .on('tick', () => {
        linkSel
          .attr('x1', d => d.source.x).attr('y1', d => d.source.y)
          .attr('x2', d => d.target.x).attr('y2', d => d.target.y);
        nodeSel.attr('transform', d => `translate(${d.x},${d.y})`);
      });
  }

  function showDetail(node) {
    document.getElementById('detail-title').textContent = node.name;
    document.getElementById('detail').innerHTML = `
      <p><strong>Type:</strong> ${node.type}</p>
      <p><strong>File:</strong> ${node.file || '?'}</p>
      <p><strong>Line:</strong> ${node.line || '?'}</p>
      ${node.complexity ? `<p><strong>Complexity:</strong> ${node.complexity}</p>` : ''}
    `;
  }

  async function expandNeighbors(node) {
    if (expanded.has(node.id)) return;
    expanded.add(node.id);
    const data = await fetchJson(`/api/node/${node.id}/neighbors`);
    const existing = new Set(graphNodes.map(n => n.id));
    for (const n of data.neighbors) {
      if (!existing.has(n.id)) graphNodes.push(n);
    }
    for (const e of data.edges) {
      const key = `${e.from}-${e.to}`;
      if (!graphLinks.some(l => `${l.from}-${l.to}` === key)) {
        graphLinks.push({ source: e.from, target: e.to, from: e.from, to: e.to });
      }
    }
    render();
  }

  async function loadGraph() {
    const query = document.getElementById('query').value.trim() || 'all';
    const depth = document.getElementById('depth').value;
    const params = new URLSearchParams({ query, limit: '250' });
    if (depth) params.set('depth', depth);
    const data = await fetchJson('/api/graph?' + params);
    graphNodes = data.nodes;
    graphLinks = data.edges.map(e => ({ ...e, source: e.from, target: e.to }));
    const typeFilter = document.getElementById('type-filter').value;
    if (typeFilter) {
      graphNodes = graphNodes.filter(n => n.type === typeFilter);
      const ids = new Set(graphNodes.map(n => n.id));
      graphLinks = graphLinks.filter(l => ids.has(l.from) && ids.has(l.to));
    }
    expanded.clear();
    initSvg();
    render();
  }

  document.getElementById('search').addEventListener('input', (e) => {
    const q = e.target.value.toLowerCase();
    nodeSel?.style('opacity', d => !q || d.name.toLowerCase().includes(q) ? 1 : 0.15);
    linkSel?.style('opacity', d => {
      const s = typeof d.source === 'object' ? d.source.name : '';
      const t = typeof d.target === 'object' ? d.target.name : '';
      return !q || s.toLowerCase().includes(q) || t.toLowerCase().includes(q) ? 1 : 0.1;
    });
  });

  window.loadGraph = loadGraph;
  window.addEventListener('resize', () => simulation?.force('center', d3.forceCenter(width() / 2, height() / 2)).alpha(0.3).restart());
  loadGraph().catch(err => {
    document.getElementById('detail').textContent = 'Error: ' + err.message;
  });
})();
