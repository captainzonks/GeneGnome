// ============================================================================
// PRIVACY NOTICE
// ============================================================================
// This application processes ALL data CLIENT-SIDE in your browser.
//
// IMPORTANT PRIVACY GUARANTEES:
// - NO data is ever sent to any server
// - NO network requests are made with your genetic data
// - NO analytics or tracking of genetic information
// - All processing happens locally in WebAssembly
// - File reading uses browser FileReader API (local only)
// - Downloads use Blob URLs (local browser memory only)
//
// Your genetic data never leaves your device. Period.
// ============================================================================

// State - All stored in browser memory only, never transmitted
let genomeData = null;
let summaryData = null;
let vcfData = null;

// Replace static imports with runtime binding so we can detect missing exports
let analyze_genome, lookup_snp, chromosome_stats, generate_vcf;
let generate_vcf_with_reference, generate_batch_vcf_with_reference, load_reference_database;
let wasmInitFn = null;
let referenceDbLoaded = false;

// Initialize WASM module
async function initWasm() {
    try {
        // Add version parameter for cache busting on updates
        // Build timestamp ensures fresh WASM on each rebuild
        const mod = await import(`./stisty_wasm.js?v=20251015-realsamples`);
        // Show what's actually exported (helps debug remotely-hosted modules)
        console.log('WASM exports:', Object.keys(mod));

        // Assign functions if present (try both snake_case and camelCase variants)
        analyze_genome = mod.analyze_genome || mod.analyzeGenome || null;
        lookup_snp = mod.lookup_snp || mod.lookupSnp || null;
        chromosome_stats = mod.chromosome_stats || mod.chromosomeStats || null;
        generate_vcf = mod.generate_vcf || mod.generateVcf || null;
        generate_vcf_with_reference = mod.generate_vcf_with_reference || null;
        generate_batch_vcf_with_reference = mod.generate_batch_vcf_with_reference || null;
        load_reference_database = mod.load_reference_database || null;

        if (!generate_vcf) {
            console.warn('generate_vcf is not exported by stisty_wasm.js — VCF export will be unavailable');
        }

        // Call initialization: use default export which is __wbg_init (the async loader)
        // Do NOT use mod.init() - that's a different function that gets called after WASM loads
        if (typeof mod.default === 'function') {
            await mod.default();
        } else {
            throw new Error('No default init function found in WASM module');
        }

        console.log('✅ WASM module initialized');

        // Load reference database for proper REF/ALT alleles
        if (load_reference_database) {
            try {
                console.log('Loading reference database...');
                const stats = await load_reference_database('./reference_db.bin.br');
                console.log('✅ Reference database loaded:', stats);
                referenceDbLoaded = true;

                // Update UI to show reference database is available
                const vcfSection = document.querySelector('.vcf-section');
                if (vcfSection) {
                    const refInfo = document.createElement('div');
                    refInfo.className = 'info-box success-box';
                    refInfo.style.marginBottom = '1rem';
                    refInfo.innerHTML = `
                        <p><strong>✅ Reference Database Loaded</strong></p>
                        <p style="font-size: 0.9em; margin-top: 0.5rem;">
                            VCF exports will use proper REF/ALT alleles from GRCh37 reference genome.
                            Ready for Michigan Imputation Server!
                        </p>
                    `;
                    vcfSection.insertBefore(refInfo, vcfSection.firstChild.nextSibling);
                }
            } catch (error) {
                console.warn('Failed to load reference database:', error);
                console.warn('VCF export will use fallback mode (not suitable for imputation)');
            }
        }
    } catch (error) {
        console.error('❌ Failed to initialize WASM:', error);
        alert('Failed to initialize the application. Please refresh the page.');
    }
}

// File handling
const fileInput = document.getElementById('fileInput');
const fileName = document.getElementById('fileName');
const loadingIndicator = document.getElementById('loadingIndicator');
const resultsSection = document.getElementById('resultsSection');

fileInput.addEventListener('change', async (event) => {
    const file = event.target.files[0];
    if (!file) return;

    fileName.textContent = file.name;

    try {
        loadingIndicator.classList.remove('hidden');
        resultsSection.classList.add('hidden');

        // PRIVACY: Read file content locally using browser FileReader API
        // This file content NEVER leaves your browser
        const text = await file.text();
        genomeData = text;

        // Analyze genome
        const resultJson = analyze_genome(text);
        summaryData = JSON.parse(resultJson);

        // Display results
        displaySummary(summaryData);

        loadingIndicator.classList.add('hidden');
        resultsSection.classList.remove('hidden');
    } catch (error) {
        console.error('Analysis error:', error);
        alert(`Failed to analyze genome data: ${error.message}`);
        loadingIndicator.classList.add('hidden');
    }
});

// Display summary
function displaySummary(data) {
    // Update stats
    document.getElementById('totalSnps').textContent = data.total_snps.toLocaleString();

    const hetRate = (data.heterozygosity_rate * 100).toFixed(2);
    document.getElementById('hetRate').textContent = `${hetRate}%`;

    document.getElementById('tsTvRatio').textContent = data.ts_tv_ratio.toFixed(4);

    // Display allele frequencies
    displayAlleleChart(data.allele_frequencies);

    // Display chromosome counts
    displayChromosomeChart(data.chromosome_counts);
}

// Display allele frequency bar chart
function displayAlleleChart(frequencies) {
    const container = document.getElementById('alleleChart');
    container.innerHTML = '';

    const sortedFreqs = frequencies.sort((a, b) => b[1] - a[1]);
    const maxFreq = Math.max(...sortedFreqs.map(f => f[1]));

    const chart = document.createElement('div');
    chart.className = 'bar-chart';

    sortedFreqs.forEach(([allele, freq]) => {
        const percentage = (freq * 100).toFixed(2);
        const barWidth = (freq / maxFreq * 100).toFixed(1);

        const item = document.createElement('div');
        item.className = 'bar-item';
        item.innerHTML = `
            <div class="bar-label">${allele}</div>
            <div class="bar-container">
                <div class="bar-fill" style="width: ${barWidth}%">${percentage}%</div>
            </div>
        `;
        chart.appendChild(item);
    });

    container.appendChild(chart);
}

// Display chromosome counts bar chart
function displayChromosomeChart(counts) {
    const container = document.getElementById('chromosomeChart');
    container.innerHTML = '';

    // Sort chromosomes numerically
    const sortedCounts = counts.sort((a, b) => {
        const aNum = parseInt(a[0]);
        const bNum = parseInt(b[0]);
        if (!isNaN(aNum) && !isNaN(bNum)) {
            return aNum - bNum;
        }
        return a[0].localeCompare(b[0]);
    });

    const maxCount = Math.max(...sortedCounts.map(c => c[1]));

    const chart = document.createElement('div');
    chart.className = 'bar-chart';

    sortedCounts.forEach(([chr, count]) => {
        const barWidth = (count / maxCount * 100).toFixed(1);

        const item = document.createElement('div');
        item.className = 'bar-item';
        item.innerHTML = `
            <div class="bar-label">Chr ${chr}</div>
            <div class="bar-container">
                <div class="bar-fill" style="width: ${barWidth}%">${count.toLocaleString()}</div>
            </div>
        `;
        chart.appendChild(item);
    });

    container.appendChild(chart);
}

// Tab switching
const tabButtons = document.querySelectorAll('.tab-button');
const tabContents = document.querySelectorAll('.tab-content');

tabButtons.forEach(button => {
    button.addEventListener('click', () => {
        const tabName = button.dataset.tab;

        // Update active states
        tabButtons.forEach(btn => btn.classList.remove('active'));
        tabContents.forEach(content => content.classList.remove('active'));

        button.classList.add('active');
        document.getElementById(`${tabName}Tab`).classList.add('active');
    });
});

// SNP Lookup
const snpInput = document.getElementById('snpInput');
const lookupButton = document.getElementById('lookupButton');
const snpResults = document.getElementById('snpResults');

lookupButton.addEventListener('click', async () => {
    if (!genomeData) {
        alert('Please upload a genome file first');
        return;
    }

    const rsid = snpInput.value.trim();
    if (!rsid) {
        alert('Please enter an SNP rsid');
        return;
    }

    try {
        const resultJson = lookup_snp(genomeData, rsid);
        const result = JSON.parse(resultJson);

        if (result === null) {
            snpResults.innerHTML = '<p style="color: var(--warning-color);">SNP not found in your genome data.</p>';
        } else {
            snpResults.innerHTML = `
                <div class="snp-info">
                    <div class="snp-field">
                        <label>rsID</label>
                        <div class="value">${result.rsid}</div>
                    </div>
                    <div class="snp-field">
                        <label>Chromosome</label>
                        <div class="value">${result.chromosome}</div>
                    </div>
                    <div class="snp-field">
                        <label>Position</label>
                        <div class="value">${result.position.toLocaleString()}</div>
                    </div>
                    <div class="snp-field">
                        <label>Genotype</label>
                        <div class="value">${result.genotype}</div>
                    </div>
                    <div class="snp-field">
                        <label>Type</label>
                        <div class="value">${result.is_heterozygous ? 'Heterozygous' : 'Homozygous'}</div>
                    </div>
                </div>
            `;
        }

        snpResults.classList.remove('hidden');
    } catch (error) {
        console.error('Lookup error:', error);
        alert(`Failed to lookup SNP: ${error.message}`);
    }
});

// Enter key for SNP lookup
snpInput.addEventListener('keypress', (event) => {
    if (event.key === 'Enter') {
        lookupButton.click();
    }
});

// Chromosome stats
const chrSelect = document.getElementById('chrSelect');
const chrResults = document.getElementById('chrResults');

chrSelect.addEventListener('change', async () => {
    if (!genomeData) {
        alert('Please upload a genome file first');
        return;
    }

    const chr = chrSelect.value;
    if (!chr) {
        chrResults.classList.add('hidden');
        return;
    }

    try {
        const resultJson = chromosome_stats(genomeData, chr);
        const result = JSON.parse(resultJson);

        const hetRate = (result.heterozygosity_rate * 100).toFixed(2);

        chrResults.innerHTML = `
            <div class="snp-info">
                <div class="snp-field">
                    <label>Chromosome</label>
                    <div class="value">${result.chromosome}</div>
                </div>
                <div class="snp-field">
                    <label>Total SNPs</label>
                    <div class="value">${result.total_snps.toLocaleString()}</div>
                </div>
                <div class="snp-field">
                    <label>Heterozygous SNPs</label>
                    <div class="value">${result.heterozygous_count.toLocaleString()}</div>
                </div>
                <div class="snp-field">
                    <label>Heterozygosity Rate</label>
                    <div class="value">${hetRate}%</div>
                </div>
            </div>
        `;

        chrResults.classList.remove('hidden');
    } catch (error) {
        console.error('Chromosome stats error:', error);
        alert(`Failed to get chromosome statistics: ${error.message}`);
    }
});

// VCF Export
const generateVcfButton = document.getElementById('generateVcfButton');
const downloadVcfButton = document.getElementById('downloadVcfButton');
const downloadBatchVcfButton = document.getElementById('downloadBatchVcfButton');
const showVcfButton = document.getElementById('showVcfButton');
const hideVcfButton = document.getElementById('hideVcfButton');
const vcfSampleName = document.getElementById('vcfSampleName');
const vcfChrSelect = document.getElementById('vcfChrSelect');
const vcfLoading = document.getElementById('vcfLoading');
const vcfSuccess = document.getElementById('vcfSuccess');
const vcfStats = document.getElementById('vcfStats');
const vcfDisplay = document.getElementById('vcfDisplay');
const vcfContent = document.getElementById('vcfContent');

generateVcfButton.addEventListener('click', async () => {
    if (!genomeData) {
        alert('Please upload a genome file first');
        return;
    }

    try {
        vcfLoading.classList.remove('hidden');
        vcfSuccess.classList.add('hidden');
        vcfDisplay.classList.add('hidden');

        const chromosome = vcfChrSelect.value; // Empty string for all chromosomes

        // Get estimated SNP count for progress messaging
        const chrText = chromosome ? ` chromosome ${chromosome}` : ' all chromosomes';
        const loadingText = vcfLoading.querySelector('p');
        const estimatedSnps = chromosome ?
            (summaryData?.chromosome_counts?.find(([chr]) => chr === chromosome)?.[1] || 'many') :
            summaryData?.total_snps || 'many';
        const snpsText = typeof estimatedSnps === 'number' ? estimatedSnps.toLocaleString() : estimatedSnps;
        loadingText.textContent = `Generating VCF for${chrText}... (${snpsText} SNPs)`;

        // Use setTimeout to allow the UI to update before blocking WASM call
        await new Promise(resolve => setTimeout(resolve, 50));

        const startTime = performance.now();
        // Use reference-aware VCF generation if available
        if (referenceDbLoaded && generate_vcf_with_reference) {
            console.log('Using reference-aware VCF generation');
            vcfData = generate_vcf_with_reference(genomeData, chromosome);
        } else {
            console.log('Using fallback VCF generation (no reference database)');
            vcfData = generate_vcf(genomeData, chromosome);
        }
        const duration = ((performance.now() - startTime) / 1000).toFixed(1);

        console.log(`✅ VCF generated in ${duration}s`);

        // Update success message with stats
        const variantCount = vcfData.split('\n').filter(line => !line.startsWith('#') && line.trim().length > 0).length;
        vcfStats.textContent = `(${variantCount.toLocaleString()} variants in ${duration}s)`;

        vcfLoading.classList.add('hidden');
        vcfSuccess.classList.remove('hidden');
    } catch (error) {
        console.error('VCF generation error:', error);
        const chrContext = vcfChrSelect.value ? ` for chromosome ${vcfChrSelect.value}` : ' for all chromosomes';
        alert(`Failed to generate VCF${chrContext}: ${error.message}`);
        vcfLoading.classList.add('hidden');
    }
});

showVcfButton.addEventListener('click', () => {
    if (!vcfData) {
        alert('Please generate VCF first');
        return;
    }

    // Format VCF for display with aligned columns
    const formattedVcf = formatVcfForDisplay(vcfData);
    vcfContent.textContent = formattedVcf;
    vcfDisplay.classList.remove('hidden');

    // Scroll to the display
    vcfDisplay.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
});

// Format VCF data with proper column alignment for display
function formatVcfForDisplay(vcfText) {
    const VCF_COLUMN_COUNT = 10;
    const COLUMN_PADDING = 2;

    const lines = vcfText.split('\n');
    const formattedLines = [];

    // Find where data lines start (after header)
    let dataStartIndex = -1;
    for (let i = 0; i < lines.length; i++) {
        if (lines[i].startsWith('#CHROM')) {
            dataStartIndex = i;
            break;
        }
    }

    if (dataStartIndex === -1) {
        // No data header found, return as-is
        return vcfText;
    }

    // Keep all header lines as-is
    for (let i = 0; i < dataStartIndex; i++) {
        formattedLines.push(lines[i]);
    }

    // Process data lines (header + variants)
    const dataLines = lines.slice(dataStartIndex).filter(line => line.trim().length > 0);

    // Calculate max width for each column
    const columnWidths = new Array(VCF_COLUMN_COUNT).fill(0);

    dataLines.forEach(line => {
        const fields = line.split('\t');
        fields.forEach((field, i) => {
            if (i < columnWidths.length) {
                columnWidths[i] = Math.max(columnWidths[i], field.length);
            }
        });
    });

    // Format each line with padding
    dataLines.forEach(line => {
        const fields = line.split('\t');
        const paddedFields = fields.map((field, i) => {
            if (i < columnWidths.length - 1) {
                // Pad all columns except the last one
                return field.padEnd(columnWidths[i] + COLUMN_PADDING, ' ');
            }
            return field; // Last column doesn't need padding
        });
        formattedLines.push(paddedFields.join(''));
    });

    return formattedLines.join('\n');
}

hideVcfButton.addEventListener('click', () => {
    vcfDisplay.classList.add('hidden');
});

downloadVcfButton.addEventListener('click', () => {
    if (!vcfData) {
        alert('Please generate VCF first');
        return;
    }

    // PRIVACY: Create a Blob URL from local browser memory only
    // No server upload - file is downloaded directly from your browser
    const blob = new Blob([vcfData], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);

    // Create a temporary link and trigger download
    const a = document.createElement('a');
    a.href = url;

    // Generate filename based on sample name and chromosome selection
    const sampleName = (vcfSampleName.value.trim() || 'mygenome').replace(/[^a-zA-Z0-9_-]/g, '_');
    const chr = vcfChrSelect.value;
    const chrSuffix = chr ? `_chr${chr}` : '_all';
    a.download = `${sampleName}${chrSuffix}.vcf`;

    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);

    // Clean up the URL object
    URL.revokeObjectURL(url);
});

// Batch VCF Download (chromosomes 1-22 as separate gzipped files in a ZIP)
downloadBatchVcfButton.addEventListener('click', async () => {
    if (!genomeData) {
        alert('Please upload a genome file first');
        return;
    }

    if (!referenceDbLoaded || !generate_batch_vcf_with_reference) {
        alert('Batch VCF export requires reference database. Please reload the page.');
        return;
    }

    try {
        // Show loading state
        const originalText = downloadBatchVcfButton.textContent;
        downloadBatchVcfButton.disabled = true;
        downloadBatchVcfButton.textContent = 'Generating...';

        console.log('Generating batch VCF files for chromosomes 1-22...');
        const startTime = performance.now();

        // Generate all chromosome VCFs
        const batchResultJson = generate_batch_vcf_with_reference(genomeData);
        const vcfFiles = JSON.parse(batchResultJson);

        const duration = ((performance.now() - startTime) / 1000).toFixed(1);
        console.log(`✅ Generated ${Object.keys(vcfFiles).length} VCF files in ${duration}s`);

        // Update button text
        downloadBatchVcfButton.textContent = 'Compressing...';

        // Use JSZip to create ZIP file with VCFs
        // Note: Michigan Imputation Server requires BGZF compression (not standard gzip)
        // BGZF is not available in browsers, so we'll provide uncompressed VCFs
        // Users can compress with bgzip locally if needed: bgzip file.vcf
        const { default: JSZip } = await import('https://cdn.jsdelivr.net/npm/jszip@3.10.1/+esm');

        const zip = new JSZip();

        // Get sample name for filename (sanitize for filesystem safety)
        const sampleName = (vcfSampleName.value.trim() || 'mygenome').replace(/[^a-zA-Z0-9_-]/g, '_');

        // Add each chromosome VCF (uncompressed - user can bgzip locally)
        // Filename format: B.{custom_name}_merged_51samples_chr{#}.vcf (matches R script naming)
        for (const [chr, vcfContent] of Object.entries(vcfFiles)) {
            zip.file(`B.${sampleName}_merged_51samples_chr${chr}.vcf`, vcfContent);
        }

        // Add README with instructions
        const readmeContent = `Michigan Imputation Server - VCF Files
========================================

Your genome has been exported to ${Object.keys(vcfFiles).length} VCF files (one per chromosome).

IMPORTANT: These files must be compressed with BGZIP before uploading to the imputation server.

Quick Start
-----------
1. Extract all files from this ZIP

2. Run the compression script for your operating system:

   🪟 Windows (CMD):
      Double-click compress_vcf.bat

   🪟 Windows (PowerShell):
      Right-click compress_vcf.ps1 -> Run with PowerShell
      Or in PowerShell: .\\compress_vcf.ps1

   🍎 macOS / 🐧 Linux:
      chmod +x compress_vcf.sh
      ./compress_vcf.sh

3. Upload the .vcf.gz files to: https://imputationserver.sph.umich.edu/

Manual Compression (Advanced)
------------------------------
If you prefer to compress manually:

   Windows (CMD):       for %f in (B.${sampleName}_merged_51samples_chr*.vcf) do bgzip "%f"
   Windows (PowerShell): Get-ChildItem *.vcf | ForEach-Object { bgzip $_.FullName }
   macOS/Linux:         for f in B.${sampleName}_merged_51samples_chr*.vcf; do bgzip "$f"; done

Installing bgzip
----------------
🐧 Ubuntu/Debian:    sudo apt install tabix
🍎 macOS:            brew install htslib
🪟 Windows:          Install WSL (sudo apt install tabix) OR
                     Conda (conda install -c bioconda htslib) OR
                     Download: https://github.com/samtools/htslib/releases

About These Files
-----------------
- Format: VCF 4.2 (Variant Call Format)
- Reference: GRCh37/hg19 coordinates
- Samples: 51 (50 anonymous + your genome)
- Filename: B.{name}_merged_51samples_chr{#}.vcf
- Quality: Filtered for biallelic SNPs with valid REF/ALT alleles
- Sorted: By chromosome and position
- Compatible: Michigan Imputation Server 2

Questions?
----------
Visit: https://imputationserver.readthedocs.io/
`;

        zip.file('README.txt', readmeContent);

        // Add compression helper script
        const compressScript = `#!/bin/bash
# Compress VCF files for Michigan Imputation Server
set -e

echo "==> Compressing VCF files with BGZIP"
echo ""

if ! command -v bgzip &> /dev/null; then
    echo "❌ Error: bgzip not found"
    echo ""
    echo "Install bgzip:"
    echo "  Ubuntu/Debian: sudo apt install tabix"
    echo "  macOS: brew install htslib"
    exit 1
fi

vcf_files=(*.vcf)
if [ \${#vcf_files[@]} -eq 0 ] || [ ! -f "\${vcf_files[0]}" ]; then
    echo "❌ No VCF files found"
    exit 1
fi

echo "Found \${#vcf_files[@]} VCF file(s)"
echo ""

for vcf in "\${vcf_files[@]}"; do
    if [ -f "$vcf" ]; then
        echo "Compressing: $vcf"
        bgzip -f "$vcf"
        echo "  ✅ Created: \${vcf}.gz"
    fi
done

echo ""
echo "==> Complete! Upload .vcf.gz files to:"
echo "    https://imputationserver.sph.umich.edu/"
`;

        zip.file('compress_vcf.sh', compressScript);

        // Add Windows batch script
        const compressScriptBat = `@echo off
REM Compress VCF files for Michigan Imputation Server (Windows)
setlocal enabledelayedexpansion

echo =^> Compressing VCF files with BGZIP
echo.

REM Check if bgzip is available
where bgzip >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo Error: bgzip not found
    echo.
    echo Install bgzip via one of these methods:
    echo   1. Install WSL ^(Windows Subsystem for Linux^) and run: sudo apt install tabix
    echo   2. Install via Conda: conda install -c bioconda htslib
    echo   3. Download from: https://github.com/samtools/htslib/releases
    echo.
    pause
    exit /b 1
)

REM Count VCF files
set count=0
for %%f in (*.vcf) do set /a count+=1

if %count% EQU 0 (
    echo Error: No VCF files found
    pause
    exit /b 1
)

echo Found %count% VCF file^(s^)
echo.

REM Compress each VCF file
for %%f in (*.vcf) do (
    echo Compressing: %%f
    bgzip -f "%%f"
    echo   Done: %%f.gz
)

echo.
echo =^> Complete! Upload .vcf.gz files to:
echo    https://imputationserver.sph.umich.edu/
echo.
pause
`;

        zip.file('compress_vcf.bat', compressScriptBat);

        // Add Windows PowerShell script
        const compressScriptPs1 = `#!/usr/bin/env pwsh
# Compress VCF files for Michigan Imputation Server (PowerShell)

Write-Host "==> Compressing VCF files with BGZIP" -ForegroundColor Cyan
Write-Host ""

# Check if bgzip is available
$bgzipExists = Get-Command bgzip -ErrorAction SilentlyContinue
if (-not $bgzipExists) {
    Write-Host "Error: bgzip not found" -ForegroundColor Red
    Write-Host ""
    Write-Host "Install bgzip via one of these methods:"
    Write-Host "  1. Install WSL (Windows Subsystem for Linux) and run: sudo apt install tabix"
    Write-Host "  2. Install via Conda: conda install -c bioconda htslib"
    Write-Host "  3. Download from: https://github.com/samtools/htslib/releases"
    Write-Host ""
    Read-Host "Press Enter to exit"
    exit 1
}

# Find VCF files
$vcfFiles = Get-ChildItem -Filter "*.vcf"
if ($vcfFiles.Count -eq 0) {
    Write-Host "Error: No VCF files found" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

Write-Host "Found $($vcfFiles.Count) VCF file(s)"
Write-Host ""

# Compress each file
foreach ($vcf in $vcfFiles) {
    Write-Host "Compressing: $($vcf.Name)" -ForegroundColor Yellow
    bgzip -f $vcf.FullName
    Write-Host "  Done: $($vcf.Name).gz" -ForegroundColor Green
}

Write-Host ""
Write-Host "==> Complete! Upload .vcf.gz files to:" -ForegroundColor Cyan
Write-Host "    https://imputationserver.sph.umich.edu/" -ForegroundColor Cyan
Write-Host ""
Read-Host "Press Enter to exit"
`;

        zip.file('compress_vcf.ps1', compressScriptPs1);

        // Generate the ZIP file
        downloadBatchVcfButton.textContent = 'Creating ZIP...';
        const zipBlob = await zip.generateAsync({ type: 'blob' });

        // Create download link
        const url = URL.createObjectURL(zipBlob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `${sampleName}_chr1-22_vcf.zip`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);

        // Reset button
        downloadBatchVcfButton.textContent = originalText;
        downloadBatchVcfButton.disabled = false;

        console.log(`✅ ZIP file created with ${Object.keys(vcfFiles).length} VCF files`);
        const fileCount = Object.keys(vcfFiles).length;
        alert(`Successfully downloaded ${fileCount} chromosome VCF files!\n\n` +
              `IMPORTANT: Michigan Imputation Server requires BGZF compression.\n\n` +
              `Next Steps:\n` +
              `  1. Extract the ZIP file\n` +
              `  2. Run the compression script for your OS:\n` +
              `     • Windows: Double-click compress_vcf.bat\n` +
              `     • PowerShell: .\\compress_vcf.ps1\n` +
              `     • Mac/Linux: chmod +x compress_vcf.sh && ./compress_vcf.sh\n` +
              `  3. Upload the .vcf.gz files to the imputation server\n\n` +
              `See README.txt in the ZIP for detailed instructions.`);

    } catch (error) {
        console.error('Batch VCF download error:', error);
        alert(`Failed to create batch VCF download: ${error.message}`);
        downloadBatchVcfButton.textContent = 'Download All Chr1-22 (ZIP)';
        downloadBatchVcfButton.disabled = false;
    }
});

// Dark Mode Toggle
const darkModeToggle = document.getElementById('darkModeToggle');
if (darkModeToggle) {
    // Load saved preference
    const savedTheme = localStorage.getItem('theme');
    if (savedTheme === 'dark') {
        document.body.classList.add('dark-mode');
    }

    // Toggle on button click
    darkModeToggle.addEventListener('click', () => {
        document.body.classList.toggle('dark-mode');

        // Save preference
        if (document.body.classList.contains('dark-mode')) {
            localStorage.setItem('theme', 'dark');
        } else {
            localStorage.setItem('theme', 'light');
        }
    });
}

// Initialize
initWasm();