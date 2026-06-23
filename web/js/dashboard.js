let complexityChart, typesChart, langChart, communityChart, communitiesBubbleChart, centralityChart;

const chartColors = {
  text: '#e6edf3',
  muted: '#8b949e',
  accent: '#58a6ff',
  green: '#3fb950',
  purple: '#bc8cff',
  palette: ['#58a6ff', '#bc8cff', '#3fb950', '#d29922', '#f85149', '#8b949e', '#79c0ff', '#ffa657']
};

const chartDefaults = {
  responsive: true,
  animation: { duration: 400 },
  plugins: {
    legend: { labels: { color: chartColors.text } }
  },
  scales: {
    x: { ticks: { color: chartColors.muted }, grid: { color: '#21262d' } },
    y: { ticks: { color: chartColors.muted }, grid: { color: '#21262d' } }
  }
};

async function fetchJson(path) {
  const r = await fetch(path);
  if (!r.ok) throw new Error(await r.text());
  return r.json();
}

function renderStats(data) {
  document.getElementById('stats').innerHTML = `
    <div class="stat-row"><span>Nodes</span><strong>${data.node_count}</strong></div>
    <div class="stat-row"><span>Functions</span><strong>${data.function_count}</strong></div>
    <div class="stat-row"><span>Classes</span><strong>${data.class_count}</strong></div>
    <div class="stat-row"><span>Files</span><strong>${data.file_count}</strong></div>
    <div class="stat-row"><span>Avg complexity</span><strong>${(data.avg_complexity || 0).toFixed(1)}</strong></div>
    <div class="stat-row"><span>Communities</span><strong>${data.community_count || 0}</strong></div>
  `;
}

function renderCommunityStats(data) {
  const modularity = data.modularity != null ? data.modularity.toFixed(3) : '—';
  const sizes = data.community_sizes || [];
  const largest = sizes.length ? Math.max(...sizes) : 0;
  const avg = sizes.length
    ? (sizes.reduce((a, b) => a + b, 0) / sizes.length).toFixed(1)
    : '—';
  document.getElementById('community-stats').innerHTML = `
    <div class="stat-row"><span>Modularity</span><strong>${modularity}</strong></div>
    <div class="stat-row"><span>Detected communities</span><strong>${data.community_count || 0}</strong></div>
    <div class="stat-row"><span>Largest community</span><strong>${largest}</strong></div>
    <div class="stat-row"><span>Avg community size</span><strong>${avg}</strong></div>
  `;
}

function upsertChart(id, existing, config) {
  const ctx = document.getElementById(id);
  if (!ctx) return existing;
  if (existing) { existing.destroy(); }
  return new Chart(ctx, config);
}

function riskClass(score) {
  if (score > 100) return 'risk-critical';
  if (score > 50) return 'risk-high';
  return 'risk-medium';
}

function renderBasicTables(data) {
  document.getElementById('connected-table').innerHTML = (data.top_connected_nodes || [])
    .map(n => `<tr>
      <td>${n.name}</td><td>${n.type}</td>
      <td>${n.in_degree}</td><td>${n.out_degree}</td>
      <td>${(n.pagerank || 0).toFixed(4)}</td>
      <td>${n.file || '?'}</td>
    </tr>`)
    .join('') || '<tr><td colspan="6">No centrality data</td></tr>';

  document.getElementById('top-table').innerHTML = (data.top_complex_functions || [])
    .map(f => `<tr><td>${f.name}</td><td>${f.complexity}</td><td>${f.file || '?'}</td></tr>`)
    .join('') || '<tr><td colspan="3">No complexity data</td></tr>';
}

function renderCommunities(communities) {
  const sorted = [...(communities || [])].sort((a, b) => b.size - a.size).slice(0, 8);
  const table = document.getElementById('communities-table');
  if (!sorted.length) {
    table.innerHTML = '<tr><td colspan="4">No communities detected</td></tr>';
    communitiesBubbleChart = upsertChart('communities-chart', communitiesBubbleChart, {
      type: 'bar',
      data: { labels: ['—'], datasets: [{ data: [0], backgroundColor: chartColors.muted }] },
      options: { ...chartDefaults, plugins: { legend: { display: false } } }
    });
    return;
  }

  table.innerHTML = sorted.map(c => `
    <tr>
      <td>${c.label}</td>
      <td>${c.size}</td>
      <td>${(c.avg_complexity || 0).toFixed(1)}</td>
      <td>${c.primary_type}</td>
    </tr>
  `).join('');

  communitiesBubbleChart = upsertChart('communities-chart', communitiesBubbleChart, {
    type: 'bubble',
    data: {
      datasets: sorted.map((c, idx) => ({
        label: c.label,
        data: [{ x: c.size, y: c.avg_complexity || 0, r: Math.max(6, Math.sqrt(c.size) * 3) }],
        backgroundColor: chartColors.palette[idx % chartColors.palette.length] + 'cc',
        borderColor: chartColors.palette[idx % chartColors.palette.length]
      }))
    },
    options: {
      ...chartDefaults,
      plugins: {
        ...chartDefaults.plugins,
        title: { display: true, text: 'Communities (size vs complexity)', color: chartColors.text }
      },
      scales: {
        x: {
          ...chartDefaults.scales.x,
          title: { display: true, text: 'Community size', color: chartColors.muted }
        },
        y: {
          ...chartDefaults.scales.y,
          title: { display: true, text: 'Avg complexity', color: chartColors.muted }
        }
      }
    }
  });
}

function renderHotspots(hotspots) {
  const table = document.getElementById('hotspots-table');
  if (!hotspots || hotspots.length === 0) {
    table.innerHTML = '<tr><td colspan="5">No hotspots detected</td></tr>';
    return;
  }
  table.innerHTML = hotspots.slice(0, 10).map((h, idx) => `
    <tr class="${riskClass(h.risk_score)}">
      <td>${idx + 1}</td>
      <td title="${h.file_path || ''}">${h.name}</td>
      <td>${h.degree}</td>
      <td>${h.complexity ?? '?'}</td>
      <td><span class="risk-badge">${(h.risk_score || 0).toFixed(0)}</span></td>
    </tr>
  `).join('');
}

function renderCentralityChart(centrality) {
  const top = (centrality || []).slice(0, 20);
  centralityChart = upsertChart('centrality-chart', centralityChart, {
    type: 'bar',
    data: {
      labels: top.map(c => c.name.length > 15 ? c.name.slice(0, 13) + '…' : c.name),
      datasets: [{
        label: 'Degree',
        data: top.map(c => c.degree),
        backgroundColor: top.map(c => {
          if (c.degree > 10) return '#f85149';
          if (c.degree > 5) return '#d29922';
          return '#3fb950';
        })
      }]
    },
    options: {
      ...chartDefaults,
      indexAxis: 'y',
      plugins: {
        ...chartDefaults.plugins,
        title: { display: true, text: 'Top 20 most connected nodes', color: chartColors.text },
        legend: { display: false }
      },
      scales: {
        x: {
          ...chartDefaults.scales.x,
          title: { display: true, text: 'Degree (connections)', color: chartColors.muted }
        },
        y: chartDefaults.scales.y
      }
    }
  });
}

async function load() {
  const [data, advanced] = await Promise.all([
    fetchJson('/api/dashboard'),
    fetchJson('/api/dashboard/advanced')
  ]);

  renderStats(data);
  renderCommunityStats(data);
  renderBasicTables(data);
  renderCommunities(advanced.communities);
  renderHotspots(advanced.hotspots);
  renderCentralityChart(advanced.centrality);

  // Load config and IaC data
  loadConfigAnalysis();
  loadIacInventory();

  const hist = data.complexity_histogram || [];
  complexityChart = upsertChart('complexity-chart', complexityChart, {
    type: 'bar',
    data: {
      labels: ['0-1', '2-5', '6-10', '11-20', '21-50', '50+'],
      datasets: [{ label: 'Functions', data: hist, backgroundColor: chartColors.accent }]
    },
    options: { ...chartDefaults, plugins: { legend: { display: false } } }
  });

  const types = data.node_types || {};
  typesChart = upsertChart('types-chart', typesChart, {
    type: 'bar',
    data: {
      labels: Object.keys(types),
      datasets: [{ data: Object.values(types), backgroundColor: chartColors.green }]
    },
    options: { ...chartDefaults, indexAxis: 'y', plugins: { legend: { display: false } } }
  });

  const langs = data.languages || {};
  langChart = upsertChart('lang-chart', langChart, {
    type: 'pie',
    data: {
      labels: Object.keys(langs),
      datasets: [{ data: Object.values(langs), backgroundColor: chartColors.palette }]
    },
    options: { plugins: { legend: { labels: { color: chartColors.text } } } }
  });

  const communities = data.communities || [];
  communityChart = upsertChart('community-chart', communityChart, {
    type: 'bar',
    data: {
      labels: communities.map(c => `#${c.id}`),
      datasets: [{ label: 'Members', data: communities.map(c => c.member_count), backgroundColor: chartColors.purple }]
    },
    options: { ...chartDefaults, plugins: { legend: { display: false } } }
  });
}

async function loadConfigAnalysis() {
  try {
    const [unused, secrets, missingEnv] = await Promise.all([
      fetchJson('/api/config/unused'),
      fetchJson('/api/config/secrets'),
      fetchJson('/api/config/missing-env')
    ]);

    // Unused keys
    document.getElementById('config-unused').innerHTML = unused.count === 0
      ? '<p style="color:#8b949e">No unused keys found</p>'
      : `<p style="margin-bottom:8px"><strong>${unused.count}</strong> unused key(s)</p>` +
        unused.keys.map(k => `<div style="padding:4px 0;border-bottom:1px solid #21262d">${k.key}<br><small style="color:#8b949e">${k.file}</small></div>`).join('');

    // Secrets
    document.getElementById('config-secrets').innerHTML = secrets.count === 0
      ? '<p style="color:#3fb950">No secrets detected</p>'
      : `<p style="margin-bottom:8px;color:#f85149"><strong>${secrets.count}</strong> finding(s)</p>` +
        secrets.findings.map(f => `<div style="padding:4px 0;border-bottom:1px solid #21262d;color:#f85149">${f.type} (line ${f.line})<br><small style="color:#8b949e">${f.file}</small></div>`).join('');

    // Missing env
    document.getElementById('config-missing-env').innerHTML = missingEnv.count === 0
      ? '<p style="color:#3fb950">All env vars defined</p>'
      : `<p style="margin-bottom:8px"><strong>${missingEnv.count}</strong> missing</p>` +
        missingEnv.variables.map(v => `<div style="padding:4px 0;border-bottom:1px solid #21262d">${v}</div>`).join('');
  } catch (err) {
    console.error('Config analysis error:', err);
    document.getElementById('config-unused').textContent = 'Error loading';
    document.getElementById('config-secrets').textContent = 'Error loading';
    document.getElementById('config-missing-env').textContent = 'Error loading';
  }
}

async function loadIacInventory() {
  try {
    const [ansible, chef, puppet] = await Promise.all([
      fetchJson('/api/iac/ansible'),
      fetchJson('/api/iac/chef'),
      fetchJson('/api/iac/puppet')
    ]);

    // Ansible
    document.getElementById('iac-ansible').innerHTML = ansible.totals.playbooks === 0
      ? '<p style="color:#8b949e">No Ansible found</p>'
      : `<div class="stat-row"><span>Playbooks</span><strong>${ansible.totals.playbooks}</strong></div>
         <div class="stat-row"><span>Plays</span><strong>${ansible.totals.plays}</strong></div>
         <div class="stat-row"><span>Tasks</span><strong>${ansible.totals.tasks}</strong></div>
         <div class="stat-row"><span>Roles</span><strong>${ansible.totals.roles}</strong></div>`;

    // Chef
    document.getElementById('iac-chef').innerHTML = chef.totals.cookbooks === 0
      ? '<p style="color:#8b949e">No Chef found</p>'
      : `<div class="stat-row"><span>Cookbooks</span><strong>${chef.totals.cookbooks}</strong></div>
         <div class="stat-row"><span>Recipes</span><strong>${chef.totals.recipes}</strong></div>
         <div class="stat-row"><span>Resources</span><strong>${chef.totals.resources}</strong></div>`;

    // Puppet
    document.getElementById('iac-puppet').innerHTML = puppet.totals.modules === 0
      ? '<p style="color:#8b949e">No Puppet found</p>'
      : `<div class="stat-row"><span>Modules</span><strong>${puppet.totals.modules}</strong></div>
         <div class="stat-row"><span>Classes</span><strong>${puppet.totals.classes}</strong></div>
         <div class="stat-row"><span>Resources</span><strong>${puppet.totals.resources}</strong></div>`;
  } catch (err) {
    console.error('IaC inventory error:', err);
    document.getElementById('iac-ansible').textContent = 'Error loading';
    document.getElementById('iac-chef').textContent = 'Error loading';
    document.getElementById('iac-puppet').textContent = 'Error loading';
  }
}

load().catch(err => {
  document.getElementById('stats').textContent = 'Error: ' + err.message;
});
