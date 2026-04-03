// Job Lookup Page Logic

class JobLookup {
    constructor() {
        this.API_BASE = window.location.origin;
        this.currentJobId = null;
        this.resendCooldownTimer = null;
        this.init();
    }

    init() {
        this.setupElements();
        this.setupEventListeners();

        // Check for job ID in URL query param
        const params = new URLSearchParams(window.location.search);
        const jobId = params.get('id');
        if (jobId) {
            this.jobIdInput.value = jobId;
            this.lookupJob();
        }
    }

    setupElements() {
        // Lookup
        this.jobIdInput = document.getElementById('jobIdInput');
        this.lookupBtn = document.getElementById('lookupBtn');

        // Sections
        this.lookupSection = document.getElementById('lookupSection');
        this.statusSection = document.getElementById('statusSection');
        this.errorSection = document.getElementById('errorSection');

        // Status details
        this.statusBadge = document.getElementById('statusBadge');
        this.detailJobId = document.getElementById('detailJobId');
        this.detailCreated = document.getElementById('detailCreated');
        this.detailStarted = document.getElementById('detailStarted');
        this.detailStartedRow = document.getElementById('detailStartedRow');
        this.detailCompleted = document.getElementById('detailCompleted');
        this.detailCompletedRow = document.getElementById('detailCompletedRow');
        this.detailExpires = document.getElementById('detailExpires');
        this.detailExpiresRow = document.getElementById('detailExpiresRow');

        // Actions
        this.resendSection = document.getElementById('resendSection');
        this.resendEmailBtn = document.getElementById('resendEmailBtn');
        this.resendCooldown = document.getElementById('resendCooldown');
        this.resendResult = document.getElementById('resendResult');

        this.deleteSection = document.getElementById('deleteSection');
        this.showDeleteBtn = document.getElementById('showDeleteBtn');
        this.deleteForm = document.getElementById('deleteForm');
        this.recoveryCodeInput = document.getElementById('recoveryCodeInput');
        this.cancelDeleteBtn = document.getElementById('cancelDeleteBtn');
        this.confirmDeleteBtn = document.getElementById('confirmDeleteBtn');
        this.deleteResult = document.getElementById('deleteResult');

        this.infoMessage = document.getElementById('infoMessage');

        // Error
        this.errorTitle = document.getElementById('errorTitle');
        this.errorMessage = document.getElementById('errorMessage');
        this.tryAgainBtn = document.getElementById('tryAgainBtn');
    }

    setupEventListeners() {
        this.lookupBtn.addEventListener('click', () => this.lookupJob());
        this.jobIdInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') this.lookupJob();
        });

        this.resendEmailBtn.addEventListener('click', () => this.resendEmail());

        this.showDeleteBtn.addEventListener('click', () => {
            this.deleteForm.classList.remove('hidden');
            this.showDeleteBtn.classList.add('hidden');
            this.recoveryCodeInput.focus();
        });

        this.cancelDeleteBtn.addEventListener('click', () => {
            this.deleteForm.classList.add('hidden');
            this.showDeleteBtn.classList.remove('hidden');
            this.recoveryCodeInput.value = '';
            this.deleteResult.classList.add('hidden');
        });

        this.confirmDeleteBtn.addEventListener('click', () => this.deleteJob());
        this.recoveryCodeInput.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') this.deleteJob();
        });

        this.tryAgainBtn.addEventListener('click', () => {
            this.errorSection.classList.add('hidden');
            this.statusSection.classList.add('hidden');
            this.lookupSection.classList.remove('hidden');
            this.jobIdInput.focus();
        });
    }

    async lookupJob() {
        const jobId = this.jobIdInput.value.trim();
        if (!jobId) {
            this.jobIdInput.focus();
            return;
        }

        // Basic UUID format validation
        const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
        if (!uuidPattern.test(jobId)) {
            this.showError('Invalid Job ID', 'Please enter a valid Job ID in UUID format (e.g. a1b2c3d4-e5f6-7890-abcd-ef1234567890).');
            return;
        }

        this.lookupBtn.classList.add('loading');
        this.lookupBtn.textContent = 'Looking up';

        try {
            const response = await fetch(`${this.API_BASE}/api/genetics/jobs/${jobId}/status`);

            if (response.status === 404) {
                this.showError('Job Not Found', 'The Job ID you entered could not be found. Please check the ID and try again.');
                return;
            }

            if (!response.ok) {
                const text = await response.text();
                this.showError('Lookup Failed', `Server returned an error: ${response.status}. ${text}`);
                return;
            }

            const data = await response.json();
            this.currentJobId = jobId;
            this.renderStatus(data);
        } catch (err) {
            this.showError('Connection Error', 'Could not reach the server. Please check your connection and try again.');
        } finally {
            this.lookupBtn.classList.remove('loading');
            this.lookupBtn.textContent = 'Look Up';
        }
    }

    renderStatus(data) {
        this.errorSection.classList.add('hidden');
        this.statusSection.classList.remove('hidden');

        // Badge
        const status = data.status;
        this.statusBadge.textContent = this.formatStatus(status);
        this.statusBadge.className = `status-badge ${status}`;

        // Details
        this.detailJobId.textContent = data.job_id;
        this.detailCreated.textContent = this.formatDate(data.created_at);

        this.toggleRow(this.detailStartedRow, data.started_at, this.detailStarted);
        this.toggleRow(this.detailCompletedRow, data.completed_at, this.detailCompleted);
        this.toggleRow(this.detailExpiresRow, data.expires_at, this.detailExpires);

        // Reset action sections
        this.resendSection.classList.add('hidden');
        this.deleteSection.classList.add('hidden');
        this.infoMessage.classList.add('hidden');
        this.deleteForm.classList.add('hidden');
        this.showDeleteBtn.classList.remove('hidden');
        this.resendResult.classList.add('hidden');
        this.deleteResult.classList.add('hidden');

        // Determine available actions based on status
        const isExpired = data.expires_at && new Date(data.expires_at) < new Date();

        if (status === 'completed' && data.has_download && !isExpired) {
            // Completed with active download: show resend + delete
            this.resendSection.classList.remove('hidden');
            this.deleteSection.classList.remove('hidden');
        } else if (status === 'completed' && isExpired) {
            this.showInfo('This job has expired. Results have been automatically deleted.');
        } else if (status === 'processing' || status === 'pending' || status === 'queued') {
            this.showInfo('Your job is still being processed. Check back in a few minutes.');
            this.deleteSection.classList.remove('hidden');
        } else if (status === 'failed') {
            this.showInfo('This job failed during processing. All uploaded data has been automatically cleaned up.');
        } else if (status === 'user_deleted') {
            this.showInfo('This job was deleted at your request. All data has been permanently removed.');
        } else {
            // Default: show delete option
            this.deleteSection.classList.remove('hidden');
        }
    }

    async resendEmail() {
        if (!this.currentJobId) return;

        this.resendEmailBtn.classList.add('loading');
        this.resendEmailBtn.textContent = 'Sending';
        this.resendResult.classList.add('hidden');

        try {
            const response = await fetch(`${this.API_BASE}/api/genetics/jobs/${this.currentJobId}/resend-email`, {
                method: 'POST',
            });

            const data = await response.json();

            if (response.ok && data.success) {
                this.resendResult.textContent = data.message || 'Download email sent successfully. Check your inbox.';
                this.resendResult.className = 'result-text success';
                this.resendResult.classList.remove('hidden');

                // Start cooldown if provided
                if (data.next_resend_available_at) {
                    this.startResendCooldown(new Date(data.next_resend_available_at));
                }
            } else if (response.status === 429) {
                this.resendResult.textContent = data.message || 'Please wait before resending.';
                this.resendResult.className = 'result-text error';
                this.resendResult.classList.remove('hidden');

                if (data.next_resend_available_at) {
                    this.startResendCooldown(new Date(data.next_resend_available_at));
                }
            } else {
                this.resendResult.textContent = data.error || data.message || 'Failed to resend email.';
                this.resendResult.className = 'result-text error';
                this.resendResult.classList.remove('hidden');
            }
        } catch (err) {
            this.resendResult.textContent = 'Connection error. Please try again.';
            this.resendResult.className = 'result-text error';
            this.resendResult.classList.remove('hidden');
        } finally {
            this.resendEmailBtn.classList.remove('loading');
            this.resendEmailBtn.textContent = 'Resend Download Email';
        }
    }

    startResendCooldown(availableAt) {
        this.resendEmailBtn.disabled = true;

        const update = () => {
            const now = new Date();
            const diff = availableAt - now;

            if (diff <= 0) {
                this.resendEmailBtn.disabled = false;
                this.resendCooldown.classList.add('hidden');
                clearInterval(this.resendCooldownTimer);
                return;
            }

            const minutes = Math.floor(diff / 60000);
            const seconds = Math.floor((diff % 60000) / 1000);
            this.resendCooldown.textContent = `You can resend in ${minutes}:${String(seconds).padStart(2, '0')}`;
            this.resendCooldown.classList.remove('hidden');
        };

        update();
        this.resendCooldownTimer = setInterval(update, 1000);
    }

    async deleteJob() {
        const code = this.recoveryCodeInput.value.trim();
        if (!code) {
            this.recoveryCodeInput.focus();
            return;
        }

        if (!this.currentJobId) return;

        this.confirmDeleteBtn.classList.add('loading');
        this.confirmDeleteBtn.textContent = 'Deleting';
        this.deleteResult.classList.add('hidden');

        try {
            const response = await fetch(`${this.API_BASE}/api/genetics/jobs/${this.currentJobId}/delete`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ recovery_code: code }),
            });

            if (response.ok) {
                this.deleteResult.textContent = 'Job deleted successfully. All data has been permanently removed.';
                this.deleteResult.className = 'result-text success';
                this.deleteResult.classList.remove('hidden');

                // Update the status badge
                this.statusBadge.textContent = 'Deleted';
                this.statusBadge.className = 'status-badge user_deleted';

                // Hide action sections
                this.resendSection.classList.add('hidden');
                this.deleteForm.classList.add('hidden');
                this.showDeleteBtn.classList.add('hidden');
            } else if (response.status === 403) {
                this.deleteResult.textContent = 'Invalid recovery code. Please check the code and try again.';
                this.deleteResult.className = 'result-text error';
                this.deleteResult.classList.remove('hidden');
            } else {
                const data = await response.json().catch(() => ({}));
                this.deleteResult.textContent = data.error || 'Failed to delete job. Please try again.';
                this.deleteResult.className = 'result-text error';
                this.deleteResult.classList.remove('hidden');
            }
        } catch (err) {
            this.deleteResult.textContent = 'Connection error. Please try again.';
            this.deleteResult.className = 'result-text error';
            this.deleteResult.classList.remove('hidden');
        } finally {
            this.confirmDeleteBtn.classList.remove('loading');
            this.confirmDeleteBtn.textContent = 'Delete My Data';
        }
    }

    showError(title, message) {
        this.statusSection.classList.add('hidden');
        this.errorSection.classList.remove('hidden');
        this.errorTitle.textContent = title;
        this.errorMessage.textContent = message;
    }

    showInfo(message) {
        this.infoMessage.textContent = message;
        this.infoMessage.classList.remove('hidden');
    }

    toggleRow(row, value, valueEl) {
        if (value) {
            row.classList.remove('hidden');
            valueEl.textContent = this.formatDate(value);
        } else {
            row.classList.add('hidden');
        }
    }

    formatStatus(status) {
        const map = {
            pending: 'Pending',
            queued: 'Queued',
            processing: 'Processing',
            completed: 'Completed',
            failed: 'Failed',
            user_deleted: 'Deleted',
        };
        return map[status] || status;
    }

    formatDate(isoString) {
        if (!isoString) return '-';
        const date = new Date(isoString);
        return date.toLocaleString(undefined, {
            year: 'numeric',
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit',
            timeZoneName: 'short',
        });
    }
}

// Initialize when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
        window.jobLookup = new JobLookup();
    });
} else {
    window.jobLookup = new JobLookup();
}
