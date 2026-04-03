// ==============================================================================
// visualize.js - Data Visualization Dashboard
// ==============================================================================
// Description: Fetches visualization data via token+password and renders charts
// Author: Matt Barham
// Created: 2026-04-02
// Version: 1.1.0
// ==============================================================================

document.addEventListener('DOMContentLoaded', () => {
    initVisualizePage();
});

// Stored visualization data (set after successful auth)
let vizData = null;

function getTokenFromURL() {
    const params = new URLSearchParams(window.location.search);
    return params.get('token');
}

function initVisualizePage() {
    const token = getTokenFromURL();

    if (!token) {
        showError('Invalid link - no token provided');
        return;
    }

    window.vizToken = token;

    document.getElementById('loadingState').style.display = 'none';
    document.getElementById('authForm').style.display = 'block';

    document.getElementById('passwordForm').addEventListener('submit', handleAuth);
}

async function handleAuth(event) {
    event.preventDefault();

    const password = document.getElementById('password').value.trim();
    const authBtn = document.getElementById('authBtn');

    if (!password) return;

    authBtn.disabled = true;
    authBtn.textContent = 'Loading...';

    try {
        const url = `/api/genetics/visualization?token=${encodeURIComponent(window.vizToken)}&password=${encodeURIComponent(password)}`;
        const response = await fetch(url);

        if (!response.ok) {
            const body = await response.json().catch(() => null);
            const message = body?.error || response.statusText;

            if (response.status === 404) {
                showError('No visualization data available for this job. This may be an older job processed before this feature was added.');
            } else {
                showError(message);
            }
            return;
        }

        vizData = await response.json();

        // Store password for download link
        window.vizPassword = password;

        document.getElementById('authForm').style.display = 'none';
        document.getElementById('dashboard').style.display = 'block';

        renderSummary(vizData);
        initializeCharts();

        // Set up download link
        const downloadLink = document.getElementById('downloadLink');
        downloadLink.href = `/download?token=${encodeURIComponent(window.vizToken)}`;

    } catch (error) {
        showError('Failed to load visualization data. Please try again.');
    } finally {
        authBtn.disabled = false;
        authBtn.textContent = 'View Insights';
    }
}

function showError(message) {
    document.getElementById('loadingState').style.display = 'none';
    document.getElementById('authForm').style.display = 'none';
    document.getElementById('errorState').style.display = 'block';
    document.getElementById('errorMessage').textContent = message;
}

function formatNumber(n) {
    return n.toLocaleString();
}

function renderSummary(data) {
    document.getElementById('totalVariants').textContent = formatNumber(data.total_variants);
    document.getElementById('genotypedVariants').textContent = formatNumber(data.genotyped_variants);
    document.getElementById('imputedVariants').textContent = formatNumber(data.imputed_variants);
    document.getElementById('chromosomesProcessed').textContent = data.chromosomes_processed;

    document.getElementById('tiTvRatio').textContent =
        data.ti_tv_ratio != null ? data.ti_tv_ratio.toFixed(3) : 'N/A';
    document.getElementById('hetRate').textContent =
        data.heterozygosity_rate != null ? (data.heterozygosity_rate * 100).toFixed(2) + '%' : 'N/A';
    document.getElementById('snpCount').textContent =
        data.variant_types ? formatNumber(data.variant_types.snps) : 'N/A';
    document.getElementById('indelCount').textContent =
        data.variant_types ? formatNumber(data.variant_types.indels) : 'N/A';
}

// Called by themes.js when the user changes themes
function reinitializeCharts() {
    Chart.helpers.each(Chart.instances, function(instance) {
        instance.destroy();
    });

    if (vizData) {
        initializeCharts();
    }
}

function getChartColors() {
    const styles = getComputedStyle(document.body);
    const isDark = document.body.classList.contains('dark-mode');
    const primary = styles.getPropertyValue('--primary-color').trim();
    const secondary = styles.getPropertyValue('--secondary-color').trim();

    return {
        primary,
        secondary,
        text: isDark ? '#f5ede3' : '#2a241f',
        textMuted: isDark ? '#b8a898' : '#5a4f45',
        grid: isDark ? '#4a3f2f' : '#d9c9b8',
        cardBg: styles.getPropertyValue('--card-bg').trim(),
        success: styles.getPropertyValue('--success-color').trim() || '#4caf50',
        warning: styles.getPropertyValue('--warning-color').trim() || '#ff9800',
    };
}

function initializeCharts() {
    if (!vizData) return;

    const c = getChartColors();

    createChromosomeChart(c);
    createSourceChart(c);
    createHistogramChart('alleleFreqChart', vizData.allele_freq_histogram, 'Variants', c);
    createHistogramChart('mafChart', vizData.maf_histogram, 'Variants', c);
    createHistogramChart('qualityChart', vizData.imputation_quality_histogram, 'Variants', c);
    createTiTvChart(c);
    createDosageChart(c);
    createVariantTypeChart(c);
}

function destroyIfExists(canvasId) {
    const canvas = document.getElementById(canvasId);
    if (!canvas) return null;
    const existing = Chart.getChart(canvas);
    if (existing) existing.destroy();
    return canvas;
}

function createChromosomeChart(c) {
    const canvas = destroyIfExists('chromosomeChart');
    if (!canvas) return;

    const chrData = vizData.per_chromosome;
    const labels = chrData.map(d => 'Chr ' + d.chromosome);

    new Chart(canvas, {
        type: 'bar',
        data: {
            labels,
            datasets: [
                {
                    label: 'Genotyped',
                    data: chrData.map(d => d.genotyped),
                    backgroundColor: c.primary,
                },
                {
                    label: 'Imputed',
                    data: chrData.map(d => d.imputed),
                    backgroundColor: c.secondary,
                },
            ],
        },
        options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: {
                    labels: { color: c.text },
                },
            },
            scales: {
                x: {
                    stacked: true,
                    ticks: { color: c.textMuted },
                    grid: { color: c.grid },
                },
                y: {
                    stacked: true,
                    ticks: { color: c.textMuted },
                    grid: { color: c.grid },
                },
            },
        },
    });
}

function createSourceChart(c) {
    const canvas = destroyIfExists('sourceChart');
    if (!canvas) return;

    const src = vizData.source_breakdown;

    new Chart(canvas, {
        type: 'doughnut',
        data: {
            labels: ['Genotyped', 'Imputed', 'Low Quality'],
            datasets: [{
                data: [src.genotyped, src.imputed, src.imputed_low_quality],
                backgroundColor: [c.primary, c.secondary, c.warning],
                borderColor: c.cardBg,
                borderWidth: 2,
            }],
        },
        options: {
            responsive: true,
            maintainAspectRatio: true,
            plugins: {
                legend: {
                    position: 'bottom',
                    labels: { color: c.text },
                },
            },
        },
    });
}

function createHistogramChart(canvasId, bins, label, c) {
    const canvas = destroyIfExists(canvasId);
    if (!canvas || !bins || bins.length === 0) return;

    new Chart(canvas, {
        type: 'bar',
        data: {
            labels: bins.map(b => b.label),
            datasets: [{
                label,
                data: bins.map(b => b.count),
                backgroundColor: c.primary,
            }],
        },
        options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: { display: false },
            },
            scales: {
                x: {
                    ticks: { color: c.textMuted, maxRotation: 45 },
                    grid: { color: c.grid },
                },
                y: {
                    ticks: { color: c.textMuted },
                    grid: { color: c.grid },
                },
            },
        },
    });
}

function createTiTvChart(c) {
    const canvas = destroyIfExists('tiTvChart');
    if (!canvas || !vizData.per_chromosome) return;

    const chrData = vizData.per_chromosome;
    const labels = chrData.map(d => 'Chr ' + d.chromosome);

    new Chart(canvas, {
        type: 'bar',
        data: {
            labels,
            datasets: [
                {
                    label: 'Ti/Tv Ratio',
                    data: chrData.map(d => d.ti_tv_ratio),
                    backgroundColor: c.primary,
                    yAxisID: 'y',
                },
                {
                    label: 'Transitions',
                    data: chrData.map(d => d.transitions),
                    backgroundColor: c.success,
                    yAxisID: 'y1',
                    hidden: true,
                },
                {
                    label: 'Transversions',
                    data: chrData.map(d => d.transversions),
                    backgroundColor: c.warning,
                    yAxisID: 'y1',
                    hidden: true,
                },
            ],
        },
        options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
                legend: {
                    labels: { color: c.text },
                },
            },
            scales: {
                x: {
                    ticks: { color: c.textMuted },
                    grid: { color: c.grid },
                },
                y: {
                    type: 'linear',
                    position: 'left',
                    title: { display: true, text: 'Ti/Tv Ratio', color: c.textMuted },
                    ticks: { color: c.textMuted },
                    grid: { color: c.grid },
                },
                y1: {
                    type: 'linear',
                    position: 'right',
                    title: { display: true, text: 'Count', color: c.textMuted },
                    ticks: { color: c.textMuted },
                    grid: { drawOnChartArea: false },
                },
            },
        },
    });
}

function createDosageChart(c) {
    const canvas = destroyIfExists('dosageChart');
    if (!canvas || !vizData.dosage_distribution) return;

    const dd = vizData.dosage_distribution;

    new Chart(canvas, {
        type: 'doughnut',
        data: {
            labels: ['Hom Ref (0)', 'Het (1)', 'Hom Alt (2)'],
            datasets: [{
                data: [dd.homozygous_ref, dd.heterozygous, dd.homozygous_alt],
                backgroundColor: [c.primary, c.secondary, c.warning],
                borderColor: c.cardBg,
                borderWidth: 2,
            }],
        },
        options: {
            responsive: true,
            maintainAspectRatio: true,
            plugins: {
                legend: {
                    position: 'bottom',
                    labels: { color: c.text },
                },
            },
        },
    });
}

function createVariantTypeChart(c) {
    const canvas = destroyIfExists('variantTypeChart');
    if (!canvas || !vizData.variant_types) return;

    const vt = vizData.variant_types;

    new Chart(canvas, {
        type: 'doughnut',
        data: {
            labels: ['SNPs', 'Indels'],
            datasets: [{
                data: [vt.snps, vt.indels],
                backgroundColor: [c.primary, c.secondary],
                borderColor: c.cardBg,
                borderWidth: 2,
            }],
        },
        options: {
            responsive: true,
            maintainAspectRatio: true,
            plugins: {
                legend: {
                    position: 'bottom',
                    labels: { color: c.text },
                },
            },
        },
    });
}
