// Progress Monitor with WebSocket Support

class ProgressMonitor {
    constructor() {
        this.ws = null;
        this.jobId = null;
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 5;
        this.reconnectDelay = 2000; // 2 seconds

        // Separate HTTP and WebSocket base URLs
        const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const httpProtocol = window.location.protocol;

        this.WS_BASE = `${wsProtocol}//${window.location.host}`;
        this.API_BASE = `${httpProtocol}//${window.location.host}`;

        this.setupElements();
        this.checkForExistingJob();
    }

    setupElements() {
        // Progress elements
        this.progressStatus = document.getElementById('progressStatus');
        this.progressPercent = document.getElementById('progressPercent');
        this.progressFill = document.getElementById('progressFill');
        this.progressDetail = document.getElementById('progressDetail');

        // Processing steps
        this.processingSteps = document.querySelectorAll('.processing-step');

        // Results elements
        this.completionTime = document.getElementById('completionTime');
        this.expiryTime = document.getElementById('expiryTime');
        this.formatCount = document.getElementById('formatCount');
        this.downloadLinks = document.getElementById('downloadLinks');
    }

    startMonitoring(jobId) {
        this.jobId = jobId;
        this.connectWebSocket();
    }

    connectWebSocket() {
        if (this.ws) {
            this.ws.close();
        }

        const wsUrl = `${this.WS_BASE}/api/genetics/jobs/${this.jobId}/ws`;
        console.log('Connecting to WebSocket:', wsUrl);

        try {
            this.ws = new WebSocket(wsUrl);

            this.ws.onopen = () => {
                console.log('WebSocket connected');
                this.reconnectAttempts = 0;
                this.updateProgress(0, 'Connected', 'Waiting for updates...');
            };

            this.ws.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data);
                    this.handleProgressUpdate(data);
                } catch (error) {
                    console.error('Failed to parse WebSocket message:', error);
                }
            };

            this.ws.onerror = (error) => {
                console.error('WebSocket error:', error);
            };

            this.ws.onclose = () => {
                console.log('WebSocket closed');
                this.handleDisconnect();
            };
        } catch (error) {
            console.error('Failed to create WebSocket:', error);
            this.fallbackToPolling();
        }
    }

    handleProgressUpdate(data) {
        console.log('Progress update:', data);

        // Handle both formats: worker format (progress_pct) and legacy format (progress)
        const progress = data.progress_pct !== undefined ? data.progress_pct : data.progress;
        const message = data.message;
        const error = data.error;

        // Infer status from progress and message
        let status = 'processing';
        if (progress >= 100) {
            status = 'completed';
        } else if (error) {
            status = 'failed';
        }

        // Update progress bar
        if (progress !== undefined) {
            this.updateProgress(progress, status, message);
        }

        // Infer step from progress percentage for visual feedback
        let step = null;
        if (progress < 20) step = 'upload';
        else if (progress < 35) step = 'validate';
        else if (progress < 80) step = 'process';
        else if (progress < 100) step = 'output';

        // Update processing step
        if (step) {
            this.updateProcessingStep(step, progress < 100 ? 'processing' : 'completed');
        }

        // Handle completion
        if (status === 'completed' || progress >= 100) {
            this.handleCompletion(data);
        }

        // Handle error
        if (status === 'failed' || error) {
            this.handleError(error || message);
        }
    }

    updateProgress(percent, status, detail) {
        this.progressPercent.textContent = `${Math.round(percent)}%`;
        this.progressFill.style.width = `${percent}%`;
        this.progressStatus.textContent = status || 'Processing...';
        if (detail) {
            this.progressDetail.textContent = detail;
        }
    }

    updateProcessingStep(stepName, status) {
        this.processingSteps.forEach(step => {
            if (step.dataset.step === stepName) {
                const statusIcon = step.querySelector('.step-status');

                step.classList.remove('active', 'completed');

                if (status === 'processing' || status === 'in_progress') {
                    step.classList.add('active');
                    statusIcon.textContent = '⏳';
                } else if (status === 'completed') {
                    step.classList.add('completed');
                    statusIcon.textContent = '✅';
                } else if (status === 'failed') {
                    statusIcon.textContent = '❌';
                }
            }
        });
    }

    async handleCompletion(data) {
        console.log('Job completed:', data);

        // Update final progress
        this.updateProgress(100, 'Complete', 'Processing finished successfully!');

        // Mark all steps as completed
        this.processingSteps.forEach(step => {
            step.classList.add('completed');
            step.querySelector('.step-status').textContent = '✅';
        });

        // Wait a moment for visual feedback
        await new Promise(resolve => setTimeout(resolve, 1000));

        // Fetch full job details
        try {
            const response = await fetch(`${this.API_BASE}/api/genetics/jobs/${this.jobId}`, {
                credentials: 'include'
            });

            if (!response.ok) {
                throw new Error('Failed to fetch job details');
            }

            const jobData = await response.json();
            this.displayResults(jobData);

        } catch (error) {
            console.error('Failed to fetch job details:', error);
            // Show results section anyway
            this.displayResults(data);
        }
    }

    displayResults(data) {
        // Switch to results section
        if (window.uploader) {
            window.uploader.showSection('results');
        }

        // Update completion time
        if (data.completed_at) {
            const completedDate = new Date(data.completed_at);
            const duration = this.calculateDuration(data.created_at, data.completed_at);
            this.completionTime.textContent = duration;

            // Calculate expiry time (72 hours from completion)
            const expiryDate = new Date(completedDate.getTime() + (72 * 60 * 60 * 1000));
            this.expiryTime.textContent = expiryDate.toLocaleString();
        }

        // Update format count
        const formats = data.output_formats || ['parquet'];
        this.formatCount.textContent = formats.length;

        // Generate download links
        this.generateDownloadLinks(formats);
    }

    generateDownloadLinks(formats) {
        this.downloadLinks.innerHTML = '';

        const formatInfo = {
            parquet: { name: 'Apache Parquet', icon: '📦', desc: 'Columnar format for analysis', size: '~240 MB' },
            json: { name: 'JSON', icon: '{ }', desc: 'Human-readable format', size: '~2.5 GB' },
            sqlite: { name: 'SQLite Database', icon: '🗄️', desc: 'Queryable database file', size: '~1.3 GB' },
            vcf: { name: 'VCF (Gzipped)', icon: '🧬', desc: 'Standard genomics format', size: '~720 MB' }
        };

        formats.forEach(format => {
            const info = formatInfo[format] || { name: format.toUpperCase(), icon: '📄', desc: '', size: '' };

            const link = document.createElement('a');
            link.className = 'download-link';
            link.href = `${this.API_BASE}/api/genetics/results/${this.jobId}?format=${format}`;
            link.download = `genetics_data.${format}`;
            link.dataset.format = format;

            link.innerHTML = `
                <div class="download-info">
                    <div class="download-icon">${info.icon}</div>
                    <div class="download-details">
                        <strong>${info.name}</strong>
                        <p>${info.desc} <span class="file-size">${info.size}</span></p>
                    </div>
                </div>
                <div class="download-action">
                    <svg class="btn-icon download-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path>
                        <polyline points="7 10 12 15 17 10"></polyline>
                        <line x1="12" y1="15" x2="12" y2="3"></line>
                    </svg>
                    <svg class="btn-icon loading-spinner" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="display: none;">
                        <circle cx="12" cy="12" r="10" opacity="0.25"></circle>
                        <path d="M12 2a10 10 0 0 1 10 10" opacity="0.75"></path>
                    </svg>
                    <span class="download-status"></span>
                </div>
            `;

            // Add click handler for visual feedback
            link.addEventListener('click', (e) => {
                console.log('Download link clicked:', format, info);
                // Don't prevent default - let browser handle download
                this.handleDownloadClick(link, info);
            });

            this.downloadLinks.appendChild(link);
            console.log('Download link created for:', format);
        });
    }

    handleDownloadClick(link, info) {
        console.log('handleDownloadClick called with:', info);

        // Show loading state
        link.classList.add('downloading');

        const downloadIcon = link.querySelector('.download-icon-svg');
        const spinner = link.querySelector('.loading-spinner');
        const status = link.querySelector('.download-status');

        console.log('DOM elements found:', {
            downloadIcon: !!downloadIcon,
            spinner: !!spinner,
            status: !!status
        });

        if (downloadIcon) {
            downloadIcon.style.display = 'none';
            console.log('Download icon hidden');
        }
        if (spinner) {
            spinner.style.display = 'block';
            spinner.style.animation = 'spin 1s linear infinite';
            console.log('Spinner shown');
        }
        if (status) {
            status.textContent = 'Preparing...';
            console.log('Status text set');
        }

        // Show notification
        this.showDownloadNotification(info);

        // Reset after delay (browser will show download dialog)
        // Large files take 10-20 seconds to start
        setTimeout(() => {
            if (downloadIcon) downloadIcon.style.display = 'block';
            if (spinner) spinner.style.display = 'none';
            if (status) status.textContent = '';
            link.classList.remove('downloading');
        }, 30000); // 30 seconds should be enough for download to start
    }

    showDownloadNotification(info) {
        console.log('showDownloadNotification called with:', info);

        // Create or update notification toast
        let toast = document.getElementById('download-toast');

        if (!toast) {
            console.log('Creating new toast element');
            toast = document.createElement('div');
            toast.id = 'download-toast';
            toast.className = 'download-toast';
            document.body.appendChild(toast);
        } else {
            console.log('Reusing existing toast element');
        }

        toast.innerHTML = `
            <div class="toast-content">
                <div class="toast-icon">⏳</div>
                <div class="toast-message">
                    <strong>Preparing download...</strong>
                    <p>Your ${info.name} file (${info.size}) is being prepared. The download will start shortly.</p>
                </div>
            </div>
        `;

        console.log('Toast HTML set, adding show class');
        toast.classList.add('show');
        console.log('Toast classes:', toast.className);

        // Hide after 8 seconds
        setTimeout(() => {
            toast.classList.remove('show');
            console.log('Toast hidden after timeout');
        }, 8000);
    }

    async handleError(errorMessage) {
        console.error('Job failed:', errorMessage);

        // Automatically clean up failed job data
        if (this.jobId) {
            console.log('Automatically cleaning up failed job:', this.jobId);
            try {
                const response = await fetch(`${this.API_BASE}/api/genetics/jobs/${this.jobId}`, {
                    method: 'DELETE',
                    credentials: 'include'
                });

                if (response.ok) {
                    console.log('Failed job data cleaned up successfully');
                } else {
                    console.warn('Failed to clean up job data, but continuing...');
                }
            } catch (error) {
                console.warn('Error during automatic cleanup:', error);
                // Continue to show error even if cleanup fails
            }
        }

        // Show error section
        if (window.uploader) {
            window.uploader.showError('Processing Failed', errorMessage);
        }

        // Close WebSocket connection
        if (this.ws) {
            this.ws.close();
        }
    }

    handleDisconnect() {
        if (this.reconnectAttempts < this.maxReconnectAttempts) {
            console.log(`Reconnecting... (attempt ${this.reconnectAttempts + 1}/${this.maxReconnectAttempts})`);
            this.reconnectAttempts++;

            setTimeout(() => {
                this.connectWebSocket();
            }, this.reconnectDelay * this.reconnectAttempts);
        } else {
            console.log('Max reconnection attempts reached, falling back to polling');
            this.fallbackToPolling();
        }
    }

    async fallbackToPolling() {
        console.log('Using polling fallback for progress updates');

        const pollInterval = 2000; // 2 seconds
        let lastStatus = null;

        const poll = async () => {
            try {
                const response = await fetch(`${this.API_BASE}/api/genetics/jobs/${this.jobId}`, {
                    credentials: 'include'
                });

                if (!response.ok) {
                    throw new Error('Failed to fetch job status');
                }

                const data = await response.json();

                // Update progress based on status (API returns "complete" not "completed")
                if (data.status === 'complete' || data.status === 'completed') {
                    this.handleCompletion(data);
                    return; // Stop polling
                } else if (data.status === 'failed') {
                    this.handleError(data.error_message || 'Job failed');
                    return; // Stop polling
                } else if (data.status === 'processing') {
                    // Estimate progress based on status changes
                    const progress = this.estimateProgress(data.status, lastStatus);
                    this.updateProgress(progress, 'Processing', 'Job in progress...');
                    lastStatus = data.status;
                }

                // Continue polling
                setTimeout(poll, pollInterval);

            } catch (error) {
                console.error('Polling error:', error);
                setTimeout(poll, pollInterval * 2); // Retry with longer delay
            }
        };

        poll();
    }

    estimateProgress(currentStatus, previousStatus) {
        const statusProgress = {
            'queued': 5,
            'validating': 20,
            'processing': 50,
            'generating': 80,
            'completed': 100,
            'failed': 0
        };

        return statusProgress[currentStatus] || 30;
    }

    calculateDuration(startTime, endTime) {
        const start = new Date(startTime);
        const end = new Date(endTime);
        const durationMs = end - start;

        const minutes = Math.floor(durationMs / 60000);
        const seconds = Math.floor((durationMs % 60000) / 1000);

        if (minutes > 0) {
            return `${minutes} min ${seconds} sec`;
        } else {
            return `${seconds} seconds`;
        }
    }

    disconnect() {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
    }

    // Check URL parameters for existing job ID and resume if found
    async checkForExistingJob() {
        const params = new URLSearchParams(window.location.search);
        const jobId = params.get('job');

        if (jobId) {
            console.log('Resuming job from URL:', jobId);

            try {
                // Fetch job status to determine what to show
                const response = await fetch(`${this.API_BASE}/api/genetics/jobs/${jobId}`, {
                    credentials: 'include'
                });

                if (!response.ok) {
                    throw new Error('Job not found');
                }

                const jobData = await response.json();
                console.log('Job data:', jobData);

                // Set jobId before any operations that need it
                this.jobId = jobId;

                // Show appropriate section based on status (API returns "complete" not "completed")
                if (jobData.status === 'complete' || jobData.status === 'completed') {
                    this.displayResults(jobData);
                    if (window.uploader) {
                        window.uploader.showSection('results');
                    }
                } else if (jobData.status === 'failed') {
                    this.handleError(jobData.error_message || 'Processing failed');
                } else {
                    // Job is still processing, show section and update with current progress
                    if (window.uploader) {
                        window.uploader.showSection('processing');
                    }

                    // Update UI with current progress state before monitoring for new updates
                    if (jobData.progress_pct !== undefined || jobData.progress !== undefined) {
                        const currentProgress = jobData.progress_pct || jobData.progress || 0;
                        const currentMessage = jobData.progress_message || jobData.message || 'Processing...';
                        this.updateProgress(currentProgress, 'processing', currentMessage);
                    }

                    this.startMonitoring(jobId);
                }
            } catch (error) {
                console.error('Failed to resume job:', error);
                // Clear invalid job ID from URL
                window.history.replaceState({}, '', window.location.pathname);
            }
        }
    }
}

// Initialize progress monitor when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
        window.progressMonitor = new ProgressMonitor();
        // Dark mode handled by darkmode.js
    });
} else {
    window.progressMonitor = new ProgressMonitor();
    // Dark mode handled by darkmode.js
}

// Dark Mode Toggle is now handled by darkmode.js (loaded in process.html)
