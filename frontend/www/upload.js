// File Upload Handler for Server-Side Processing

class GeneticsUploader {
    constructor() {
        this.files = new Map(); // filename -> File object
        // Use same-origin API (no CORS issues!)
        this.API_BASE = window.location.origin;

        this.fileTypes = {
            genome: { pattern: /^genome.*\.txt$/i, icon: '🧬', maxSize: 50 * 1024 * 1024 }, // 50MB
            vcf: { pattern: /^chr\d{1,2}\.dose\.vcf\.gz$/i, icon: '📊', maxSize: 300 * 1024 * 1024 }, // 300MB (chr1-4 can be ~250MB)
            pgs: { pattern: /\.(pgs|txt)$/i, icon: '📈', maxSize: 10 * 1024 * 1024 } // 10MB
        };

        this.init();
    }

    init() {
        this.setupElements();
        this.setupEventListeners();

        // Check for job ID in URL and store it
        const params = new URLSearchParams(window.location.search);
        const jobId = params.get('job');

        if (jobId) {
            // Store job ID for delete functionality
            this.currentJobId = jobId;
            console.log('Uploader: Job ID loaded from URL:', jobId);
            // Don't show upload section (ProgressMonitor handles display)
        } else {
            // No job in URL, show upload section
            this.showSection('upload');
        }
    }

    setupElements() {
        // Drop zone
        this.dropZone = document.getElementById('dropZone');
        this.fileInput = document.getElementById('fileInput');
        this.browseButton = document.getElementById('browseButton');

        // Email input
        this.emailSection = document.getElementById('emailSection');
        this.emailInput = document.getElementById('emailInput');

        // File list
        this.fileListContainer = document.getElementById('fileListContainer');
        this.fileList = document.getElementById('fileList');
        this.fileCount = document.getElementById('fileCount');
        this.clearAllButton = document.getElementById('clearAllButton');
        this.uploadButton = document.getElementById('uploadButton');

        // Format selection
        this.formatSection = document.getElementById('formatSection');
        this.formatCheckboxes = document.querySelectorAll('input[name="format"]');
        this.vcfFormatCheckbox = document.getElementById('vcfFormatCheckbox');
        this.vcfFormatOptions = document.getElementById('vcfFormatOptions');
        this.vcfFormatRadios = document.querySelectorAll('input[name="vcf_format"]');

        // Sections
        this.sections = {
            upload: document.getElementById('uploadSection'),
            processing: document.getElementById('processingSection'),
            results: document.getElementById('resultsSection'),
            error: document.getElementById('errorSection')
        };

        // Buttons
        this.deleteButton = document.getElementById('deleteButton');
        this.newJobButton = document.getElementById('newJobButton');
        this.retryButton = document.getElementById('retryButton');
    }

    setupEventListeners() {
        // Drop zone
        this.dropZone.addEventListener('click', () => this.fileInput.click());
        this.browseButton.addEventListener('click', (e) => {
            e.stopPropagation();
            this.fileInput.click();
        });

        this.dropZone.addEventListener('dragover', (e) => {
            e.preventDefault();
            this.dropZone.classList.add('drag-over');
        });

        this.dropZone.addEventListener('dragleave', () => {
            this.dropZone.classList.remove('drag-over');
        });

        this.dropZone.addEventListener('drop', (e) => {
            e.preventDefault();
            this.dropZone.classList.remove('drag-over');
            this.handleFiles(e.dataTransfer.files);
        });

        this.fileInput.addEventListener('change', (e) => {
            this.handleFiles(e.target.files);
        });

        // Email validation
        this.emailInput.addEventListener('input', () => {
            this.checkRequiredFiles();
        });

        // VCF format toggle
        this.vcfFormatCheckbox.addEventListener('change', () => {
            this.toggleVcfFormatOptions();
        });
        // Initialize VCF format options visibility
        this.toggleVcfFormatOptions();

        // File list actions
        this.clearAllButton.addEventListener('click', () => this.clearAll());
        this.uploadButton.addEventListener('click', () => this.startUpload());

        // Results actions
        if (this.deleteButton) {
            this.deleteButton.addEventListener('click', () => this.deleteJob());
        }
        if (this.newJobButton) {
            this.newJobButton.addEventListener('click', () => this.resetForNewJob());
        }
        if (this.retryButton) {
            this.retryButton.addEventListener('click', () => this.resetForNewJob());
        }
    }

    handleFiles(fileList) {
        Array.from(fileList).forEach(file => {
            const validation = this.validateFile(file);
            if (validation.valid || true) { // Accept all for now, show warnings
                this.files.set(file.name, file);
            }
        });
        this.updateFileList();
    }

    validateFile(file) {
        // Check file type
        let fileType = null;
        let valid = false;
        let reason = '';

        for (const [type, config] of Object.entries(this.fileTypes)) {
            if (config.pattern.test(file.name)) {
                fileType = type;
                if (file.size > config.maxSize) {
                    reason = `File too large (max ${this.formatBytes(config.maxSize)})`;
                } else {
                    valid = true;
                }
                break;
            }
        }

        if (!fileType) {
            reason = 'Unknown file type';
        }

        return { valid, fileType, reason };
    }

    updateFileList() {
        // Show/hide file list container
        if (this.files.size === 0) {
            this.emailSection.classList.add('hidden');
            this.fileListContainer.classList.add('hidden');
            this.formatSection.classList.add('hidden');
            this.uploadButton.disabled = true;
            return;
        }

        this.emailSection.classList.remove('hidden');
        this.fileListContainer.classList.remove('hidden');
        this.formatSection.classList.remove('hidden');
        this.fileCount.textContent = this.files.size;

        // Clear and rebuild file list
        this.fileList.innerHTML = '';

        this.files.forEach((file, filename) => {
            const validation = this.validateFile(file);
            const fileItem = this.createFileItem(file, validation);
            this.fileList.appendChild(fileItem);
        });

        // Check if we have required files
        this.checkRequiredFiles();
    }

    createFileItem(file, validation) {
        const item = document.createElement('div');
        item.className = `file-item ${validation.valid ? '' : 'invalid'}`;

        const icon = validation.fileType
            ? this.fileTypes[validation.fileType].icon
            : '📄';

        item.innerHTML = `
            <div class="file-info">
                <div class="file-icon">${icon}</div>
                <div class="file-details">
                    <span class="file-name">${file.name}</span>
                    <span class="file-meta">${this.formatBytes(file.size)} • ${validation.fileType || 'Unknown'}</span>
                </div>
            </div>
            <div class="file-status">
                <span class="status-badge ${validation.valid ? 'valid' : 'invalid'}">
                    ${validation.valid ? '✓ Valid' : '⚠ ' + validation.reason}
                </span>
                <button class="remove-file" data-filename="${file.name}">✕</button>
            </div>
        `;

        // Remove button
        item.querySelector('.remove-file').addEventListener('click', () => {
            this.files.delete(file.name);
            this.updateFileList();
        });

        return item;
    }

    checkRequiredFiles() {
        let hasGenome = false;
        let vcfCount = 0;

        this.files.forEach((file, filename) => {
            if (this.fileTypes.genome.pattern.test(filename)) hasGenome = true;
            if (this.fileTypes.vcf.pattern.test(filename)) vcfCount++;
        });

        // Check email validity
        const emailValid = this.emailInput.validity.valid && this.emailInput.value.trim().length > 0;

        // Enable upload if we have genome file, VCF files, and valid email
        this.uploadButton.disabled = !(hasGenome && vcfCount > 0 && emailValid);
    }

    clearAll() {
        this.files.clear();
        this.emailInput.value = '';
        this.updateFileList();
        this.fileInput.value = '';
    }

    async startUpload() {
        // Validate email
        const email = this.emailInput.value.trim();
        if (!this.emailInput.validity.valid || !email) {
            alert('Please enter a valid email address');
            return;
        }

        // Get selected formats
        const selectedFormats = Array.from(this.formatCheckboxes)
            .filter(cb => cb.checked)
            .map(cb => cb.value);

        if (selectedFormats.length === 0) {
            alert('Please select at least one output format');
            return;
        }

        // Check if we need chunked upload (any file >50MB)
        let needsChunkedUpload = false;
        for (const [filename, file] of this.files.entries()) {
            if (file.size > 50 * 1024 * 1024) { // 50MB
                needsChunkedUpload = true;
                break;
            }
        }

        // Show processing section
        this.showSection('processing');

        try {
            if (needsChunkedUpload) {
                await this.startChunkedUpload(selectedFormats, email);
            } else {
                await this.startStandardUpload(selectedFormats, email);
            }
        } catch (error) {
            console.error('Upload error:', error);
            this.showError('Upload failed', error.message);
        }
    }

    async startStandardUpload(selectedFormats, email) {
        // Create FormData
        const formData = new FormData();

        // Add email
        formData.append('user_email', email);

        // Add files
        this.files.forEach((file, filename) => {
            const validation = this.validateFile(file);
            if (validation.fileType === 'genome') {
                formData.append('genome_file', file);
            } else if (validation.fileType === 'vcf') {
                formData.append('vcf_files', file);
            } else if (validation.fileType === 'pgs') {
                formData.append('pgs_file', file);
            }
        });

        // Add output formats
        selectedFormats.forEach(format => {
            formData.append('output_formats', format);
        });

        // Add VCF format preference (merged or per_chromosome)
        const vcfFormat = Array.from(this.vcfFormatRadios).find(r => r.checked)?.value || 'merged';
        formData.append('vcf_format', vcfFormat);

        // Upload files (same-origin, no credentials needed)
        const response = await fetch(`${this.API_BASE}/api/genetics/jobs`, {
            method: 'POST',
            body: formData
        });

        if (!response.ok) {
            const errorText = await response.text();
            throw new Error(`Upload failed (${response.status}): ${errorText || response.statusText}`);
        }

        const result = await response.json();
        this.currentJobId = result.job_id;

        // Store job in localStorage and update URL
        this.saveJobToHistory(this.currentJobId);
        this.updateURLWithJobId(this.currentJobId);

        // Switch messages: hide upload warning, show processing note
        const uploadWarning = document.getElementById('uploadWarning');
        const processingNote = document.getElementById('processingNote');
        if (uploadWarning) uploadWarning.classList.add('hidden');
        if (processingNote) processingNote.classList.remove('hidden');

        // Start progress monitoring
        if (window.progressMonitor) {
            window.progressMonitor.startMonitoring(this.currentJobId);
        }
    }

    async startChunkedUpload(selectedFormats, email) {
        // Step 1: Initialize chunked upload session
        const uploadId = this.generateUploadId();

        // Add uploading animation to progress bar
        const progressFill = document.getElementById('progressFill');
        if (progressFill) {
            progressFill.classList.add('uploading');
        }

        // Calculate total size and number of chunks
        const chunkSize = 40 * 1024 * 1024; // 40MB chunks (under Cloudflare's 100MB limit)
        let totalSize = 0;
        let totalChunks = 0;
        const fileManifest = [];

        for (const [filename, file] of this.files.entries()) {
            const validation = this.validateFile(file);
            const numChunks = Math.ceil(file.size / chunkSize);

            fileManifest.push({
                filename: filename,
                fileType: validation.fileType,
                size: file.size,
                chunks: numChunks,
                startChunk: totalChunks
            });

            totalSize += file.size;
            totalChunks += numChunks;
        }

        console.log(`Chunked upload: ${totalChunks} total chunks, ${this.formatBytes(totalSize)}`);

        // Step 2: Upload all chunks with progress tracking
        let uploadedChunks = 0;

        for (const fileInfo of fileManifest) {
            const file = this.files.get(fileInfo.filename);

            for (let chunkIndex = 0; chunkIndex < fileInfo.chunks; chunkIndex++) {
                const start = chunkIndex * chunkSize;
                const end = Math.min(start + chunkSize, file.size);
                const chunk = file.slice(start, end);

                const formData = new FormData();
                formData.append('upload_id', uploadId);
                formData.append('filename', fileInfo.filename);
                formData.append('file_type', fileInfo.fileType);
                formData.append('chunk_index', chunkIndex);
                formData.append('total_chunks', fileInfo.chunks);
                formData.append('chunk', chunk);

                // Update progress
                const progressPercent = (uploadedChunks / totalChunks) * 100;
                if (window.progressMonitor) {
                    window.progressMonitor.updateProgress(
                        progressPercent,
                        'Uploading',
                        `Uploading ${fileInfo.filename} (chunk ${chunkIndex + 1}/${fileInfo.chunks})`
                    );
                }

                // Upload chunk
                const response = await fetch(`${this.API_BASE}/api/genetics/upload/chunks`, {
                    method: 'POST',
                    body: formData,
                    credentials: 'include'  // Required for Authentik session cookies
                });

                if (!response.ok) {
                    const errorText = await response.text();
                    throw new Error(`Chunk upload failed (${response.status}): ${errorText || response.statusText}`);
                }

                uploadedChunks++;
            }
        }

        // Step 3: Finalize upload and create job
        const finalizeData = new FormData();
        finalizeData.append('upload_id', uploadId);
        finalizeData.append('user_email', email);
        selectedFormats.forEach(format => {
            finalizeData.append('output_formats', format);
        });

        // Add VCF format preference (merged or per_chromosome)
        const vcfFormat = Array.from(this.vcfFormatRadios).find(r => r.checked)?.value || 'merged';
        finalizeData.append('vcf_format', vcfFormat);

        const response = await fetch(`${this.API_BASE}/api/genetics/upload/finalize`, {
            method: 'POST',
            body: finalizeData,
            credentials: 'include'  // Required for Authentik session cookies
        });

        if (!response.ok) {
            const errorText = await response.text();
            throw new Error(`Finalize failed (${response.status}): ${errorText || response.statusText}`);
        }

        const result = await response.json();
        this.currentJobId = result.job_id;

        // Store job in localStorage and update URL
        this.saveJobToHistory(this.currentJobId);
        this.updateURLWithJobId(this.currentJobId);

        // Remove uploading animation and switch to processing message
        if (progressFill) {
            progressFill.classList.remove('uploading');
        }

        // Switch messages: hide upload warning, show processing note
        const uploadWarning = document.getElementById('uploadWarning');
        const processingNote = document.getElementById('processingNote');
        if (uploadWarning) uploadWarning.classList.add('hidden');
        if (processingNote) processingNote.classList.remove('hidden');

        // Start progress monitoring
        if (window.progressMonitor) {
            window.progressMonitor.startMonitoring(this.currentJobId);
        }
    }

    generateUploadId() {
        // Generate a unique upload session ID
        return `upload_${Date.now()}_${Math.random().toString(36).substring(2, 15)}`;
    }

    showSection(section) {
        Object.values(this.sections).forEach(el => el.classList.add('hidden'));
        if (this.sections[section]) {
            this.sections[section].classList.remove('hidden');
        }
    }

    showError(title, details) {
        this.showSection('error');
        document.getElementById('errorText').textContent = title;
        if (details) {
            document.getElementById('errorDetails').textContent = details;
        }
    }

    async deleteJob() {
        console.log('deleteJob called, currentJobId:', this.currentJobId);

        if (!this.currentJobId) {
            console.warn('No current job ID, cannot delete');
            return;
        }

        try {
            // Show confirmation modal
            console.log('Showing confirmation modal...');
            const confirmed = await this.showDeleteConfirmation();
            console.log('Confirmation result:', confirmed);

            if (!confirmed) {
                console.log('Delete cancelled by user');
                return;
            }

            console.log('Sending DELETE request to:', `${this.API_BASE}/api/genetics/jobs/${this.currentJobId}`);
            const response = await fetch(`${this.API_BASE}/api/genetics/jobs/${this.currentJobId}`, {
                method: 'DELETE',
                credentials: 'include'
            });

            console.log('DELETE response status:', response.status, response.statusText);

            if (response.ok) {
                console.log('Delete successful, showing success toast');
                this.showSuccessToast('Job deleted successfully', 'All data and results have been permanently removed.');
                this.resetForNewJob();
            } else {
                const errorText = await response.text();
                console.error('Delete failed with status:', response.status, errorText);
                throw new Error(`Delete failed: ${response.status} ${errorText}`);
            }
        } catch (error) {
            console.error('Delete error:', error);
            this.showErrorToast('Failed to delete job', error.message);
        }
    }

    showDeleteConfirmation() {
        console.log('showDeleteConfirmation called');
        return new Promise((resolve) => {
            // Create modal overlay
            const modal = document.createElement('div');
            modal.className = 'confirmation-modal-overlay';
            console.log('Modal element created');
            modal.innerHTML = `
                <div class="confirmation-modal">
                    <div class="confirmation-header">
                        <div class="confirmation-icon">⚠️</div>
                        <h3>Delete Job and All Data?</h3>
                    </div>
                    <div class="confirmation-body">
                        <p><strong>This action cannot be undone.</strong></p>
                        <p>This will permanently delete:</p>
                        <ul>
                            <li>All uploaded genetic data files</li>
                            <li>All processed results (Parquet, SQLite, VCF)</li>
                            <li>Job metadata and history</li>
                        </ul>
                    </div>
                    <div class="confirmation-actions">
                        <button class="btn btn-secondary cancel-btn">Cancel</button>
                        <button class="btn btn-danger confirm-btn">Delete Everything</button>
                    </div>
                </div>
            `;

            document.body.appendChild(modal);

            // Handle button clicks
            const cancelBtn = modal.querySelector('.cancel-btn');
            const confirmBtn = modal.querySelector('.confirm-btn');

            const cleanup = () => {
                modal.classList.add('closing');
                setTimeout(() => modal.remove(), 200);
            };

            cancelBtn.addEventListener('click', () => {
                cleanup();
                resolve(false);
            });

            confirmBtn.addEventListener('click', () => {
                cleanup();
                resolve(true);
            });

            // Close on overlay click
            modal.addEventListener('click', (e) => {
                if (e.target === modal) {
                    cleanup();
                    resolve(false);
                }
            });

            // Animate in
            setTimeout(() => modal.classList.add('show'), 10);
        });
    }

    showSuccessToast(title, message) {
        this.showToast(title, message, 'success');
    }

    showErrorToast(title, message) {
        this.showToast(title, message, 'error');
    }

    showToast(title, message, type = 'success') {
        let toast = document.getElementById('action-toast');

        if (!toast) {
            toast = document.createElement('div');
            toast.id = 'action-toast';
            toast.className = 'action-toast';
            document.body.appendChild(toast);
        }

        const icon = type === 'success' ? '✓' : '✗';
        toast.className = `action-toast ${type}`;

        toast.innerHTML = `
            <div class="toast-content">
                <div class="toast-icon">${icon}</div>
                <div class="toast-message">
                    <strong>${title}</strong>
                    ${message ? `<p>${message}</p>` : ''}
                </div>
            </div>
        `;

        toast.classList.add('show');

        setTimeout(() => toast.classList.remove('show'), 5000);
    }

    resetForNewJob() {
        this.currentJobId = null;
        this.clearAll();
        this.showSection('upload');

        // Reset format checkboxes to default (Parquet + VCF)
        this.formatCheckboxes.forEach((cb, index) => {
            cb.checked = index === 0 || index === 1; // Check Parquet (0) and VCF (1)
        });

        // Clear email input
        this.emailInput.value = '';

        // Reset VCF format options
        this.toggleVcfFormatOptions();
    }

    toggleVcfFormatOptions() {
        // Show/hide VCF format suboptions based on VCF checkbox state
        if (this.vcfFormatCheckbox.checked) {
            this.vcfFormatOptions.classList.remove('hidden');
        } else {
            this.vcfFormatOptions.classList.add('hidden');
        }
    }

    formatBytes(bytes) {
        if (bytes === 0) return '0 Bytes';
        const k = 1024;
        const sizes = ['Bytes', 'KB', 'MB', 'GB'];
        const i = Math.floor(Math.log(bytes) / Math.log(k));
        return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i];
    }

    // Update URL with job ID for bookmarking/sharing
    updateURLWithJobId(jobId) {
        const url = new URL(window.location.href);
        url.searchParams.set('job', jobId);
        window.history.pushState({ jobId }, '', url);
        console.log('Job ID added to URL:', jobId);
    }

    // Save job to localStorage history (last 10 jobs)
    saveJobToHistory(jobId) {
        try {
            const history = JSON.parse(localStorage.getItem('genetics_jobs') || '[]');
            const jobRecord = {
                id: jobId,
                timestamp: new Date().toISOString(),
                url: window.location.origin + '/process.html?job=' + jobId
            };

            // Add to beginning and keep last 10
            history.unshift(jobRecord);
            const trimmed = history.slice(0, 10);

            localStorage.setItem('genetics_jobs', JSON.stringify(trimmed));
            console.log('Job saved to history:', jobId);
        } catch (e) {
            console.warn('Failed to save job to localStorage:', e);
        }
    }
}

// Initialize uploader when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
        window.uploader = new GeneticsUploader();
    });
} else {
    window.uploader = new GeneticsUploader();
}
