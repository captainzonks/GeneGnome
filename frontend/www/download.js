// ==============================================================================
// download.js - Secure Download Handler
// ==============================================================================
// Description: Handles token-based result downloads with password verification
// Author: Matt Barham
// Created: 2025-11-19
// Modified: 2025-11-19
// Version: 1.0.0
// ==============================================================================

// Initialize download page when DOM is ready
document.addEventListener('DOMContentLoaded', () => {
    initDownloadPage();
});

// Extract token from URL query parameter
function getTokenFromURL() {
    const params = new URLSearchParams(window.location.search);
    return params.get('token');
}

// Initialize the download page
function initDownloadPage() {
    const token = getTokenFromURL();

    if (!token) {
        showError('Invalid download link - no token provided');
        return;
    }

    // Store token for later use
    window.downloadToken = token;

    // Show download form
    document.getElementById('loadingState').style.display = 'none';
    document.getElementById('downloadForm').style.display = 'block';

    // Set up form handler
    const form = document.getElementById('passwordForm');
    form.addEventListener('submit', handleDownload);
}

// Handle download form submission
async function handleDownload(event) {
    event.preventDefault();

    const password = document.getElementById('password').value.trim();
    const downloadBtn = document.getElementById('downloadBtn');

    if (!password) {
        showError('Please enter your download password');
        return;
    }

    // Disable button and show loading state
    downloadBtn.disabled = true;
    downloadBtn.textContent = 'Downloading...';

    try {
        // For large files, use direct download link instead of blob to avoid memory issues
        // The browser will handle the download natively
        const downloadUrl = `/api/genetics/download?token=${encodeURIComponent(window.downloadToken)}&password=${encodeURIComponent(password)}`;

        // Create hidden link and trigger download
        const a = document.createElement('a');
        a.style.display = 'none';
        a.href = downloadUrl;
        a.download = 'genetics_results.zip';
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);

        // Show success message after a short delay
        setTimeout(() => {
            showSuccess('genetics_results.zip');
        }, 500);

    } catch (error) {
        console.error('Download error:', error);
        showError(error.message);

        // Re-enable button
        downloadBtn.disabled = false;
        downloadBtn.textContent = 'Download Results';
    }
}

// Show error message
function showError(message) {
    document.getElementById('loadingState').style.display = 'none';
    document.getElementById('downloadForm').style.display = 'none';
    document.getElementById('errorState').style.display = 'block';
    document.getElementById('errorMessage').textContent = message;
}

// Show success message
function showSuccess(filename) {
    const formDiv = document.getElementById('downloadForm');
    formDiv.innerHTML = `
        <div class="download-card-header">
            <h2>✅ Download Started!</h2>
            <p>Your file should begin downloading shortly</p>
        </div>
        <div class="download-card-form">
            <div class="info-box info-green">
                <p><strong>${filename}</strong> is downloading to your device.</p>
                <p style="margin-top: 1rem;">If the download doesn't start automatically, check your browser's download settings.</p>
            </div>
            <div class="info-box info-blue" style="margin-top: 1.5rem;">
                <h4>📁 Your Results Include:</h4>
                <ul style="margin: 0.5rem 0 0 1.5rem; padding: 0;">
                    <li><strong>Parquet/SQLite files:</strong> Optimized for data analysis</li>
                    <li><strong>VCF files:</strong> Standard genomic format</li>
                </ul>
            </div>
            <a href="/process" class="btn-primary" style="display: inline-block; text-decoration: none; width: 100%; margin-top: 1.5rem;">
                Process Another File
            </a>
        </div>
    `;
}
