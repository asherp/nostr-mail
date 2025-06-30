// Notification Service
// Handles user notifications, success messages, and error feedback

export class NotificationService {
    constructor() {
        this.notifications = [];
        this.init();
    }

    init() {
        // Add CSS animations if not already present
        if (!document.getElementById('notification-styles')) {
            const style = document.createElement('style');
            style.id = 'notification-styles';
            style.textContent = `
                @keyframes slideIn {
                    from { transform: translateX(100%); opacity: 0; }
                    to { transform: translateX(0); opacity: 1; }
                }
                @keyframes slideOut {
                    from { transform: translateX(0); opacity: 1; }
                    to { transform: translateX(100%); opacity: 0; }
                }
            `;
            document.head.appendChild(style);
        }
    }

    showSuccess(message, duration = 5000) {
        this.showNotification(message, 'success', duration);
    }

    showError(message, duration = 5000) {
        this.showNotification(message, 'error', duration);
    }

    showWarning(message, duration = 5000) {
        this.showNotification(message, 'warning', duration);
    }

    showInfo(message, duration = 5000) {
        this.showNotification(message, 'info', duration);
    }

    showNotification(message, type = 'info', duration = 5000) {
        try {
            const notification = document.createElement('div');
            notification.className = `notification notification-${type}`;
            
            const icon = this.getIconForType(type);
            
            notification.innerHTML = `
                <div class="notification-content">
                    <i class="fas fa-${icon}"></i>
                    <span>${message}</span>
                </div>
            `;
            
            // Add styles
            notification.style.cssText = `
                position: fixed;
                top: 20px;
                right: 20px;
                padding: 15px 20px;
                border-radius: 8px;
                color: white;
                font-weight: 500;
                z-index: 1001;
                animation: slideIn 0.3s ease;
                background: ${this.getBackgroundForType(type)};
                box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
                max-width: 400px;
                word-wrap: break-word;
            `;
            
            document.body.appendChild(notification);
            this.notifications.push(notification);
            
            // Remove after duration
            setTimeout(() => {
                this.removeNotification(notification);
            }, duration);
            
        } catch (error) {
            console.error('Error showing notification:', error);
            // Fallback to console
            console.log(`[${type.toUpperCase()}] ${message}`);
        }
    }

    removeNotification(notification) {
        try {
            notification.style.animation = 'slideOut 0.3s ease';
            setTimeout(() => {
                if (notification.parentNode) {
                    notification.parentNode.removeChild(notification);
                }
                // Remove from notifications array
                const index = this.notifications.indexOf(notification);
                if (index > -1) {
                    this.notifications.splice(index, 1);
                }
            }, 300);
        } catch (error) {
            console.error('Error removing notification:', error);
        }
    }

    clearAllNotifications() {
        this.notifications.forEach(notification => {
            this.removeNotification(notification);
        });
    }

    getIconForType(type) {
        switch (type) {
            case 'success':
                return 'check-circle';
            case 'error':
                return 'exclamation-circle';
            case 'warning':
                return 'exclamation-triangle';
            case 'info':
                return 'info-circle';
            default:
                return 'info-circle';
        }
    }

    getBackgroundForType(type) {
        switch (type) {
            case 'success':
                return 'linear-gradient(135deg, #28a745, #20c997)';
            case 'error':
                return 'linear-gradient(135deg, #dc3545, #fd7e14)';
            case 'warning':
                return 'linear-gradient(135deg, #ffc107, #fd7e14)';
            case 'info':
                return 'linear-gradient(135deg, #17a2b8, #6f42c1)';
            default:
                return 'linear-gradient(135deg, #6c757d, #495057)';
        }
    }

    // Show loading notification
    showLoading(message = 'Loading...') {
        const notification = document.createElement('div');
        notification.className = 'notification notification-loading';
        notification.innerHTML = `
            <div class="notification-content">
                <i class="fas fa-spinner fa-spin"></i>
                <span>${message}</span>
            </div>
        `;
        
        notification.style.cssText = `
            position: fixed;
            top: 20px;
            right: 20px;
            padding: 15px 20px;
            border-radius: 8px;
            color: white;
            font-weight: 500;
            z-index: 1001;
            animation: slideIn 0.3s ease;
            background: linear-gradient(135deg, #6c757d, #495057);
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
        `;
        
        document.body.appendChild(notification);
        return notification;
    }

    // Hide loading notification
    hideLoading(notification) {
        if (notification && notification.parentNode) {
            this.removeNotification(notification);
        }
    }

    // Show confirmation dialog
    async showConfirmation(message, title = 'Confirm') {
        return new Promise((resolve) => {
            const modal = document.createElement('div');
            modal.className = 'confirmation-modal';
            modal.innerHTML = `
                <div class="confirmation-overlay">
                    <div class="confirmation-dialog">
                        <h3>${title}</h3>
                        <p>${message}</p>
                        <div class="confirmation-actions">
                            <button class="btn btn-secondary" id="confirm-cancel">Cancel</button>
                            <button class="btn btn-primary" id="confirm-ok">OK</button>
                        </div>
                    </div>
                </div>
            `;
            
            modal.style.cssText = `
                position: fixed;
                top: 0;
                left: 0;
                width: 100%;
                height: 100%;
                z-index: 1002;
                display: flex;
                align-items: center;
                justify-content: center;
            `;
            
            const overlay = modal.querySelector('.confirmation-overlay');
            overlay.style.cssText = `
                position: absolute;
                top: 0;
                left: 0;
                width: 100%;
                height: 100%;
                background: rgba(0, 0, 0, 0.5);
                display: flex;
                align-items: center;
                justify-content: center;
            `;
            
            const dialog = modal.querySelector('.confirmation-dialog');
            dialog.style.cssText = `
                background: white;
                padding: 20px;
                border-radius: 8px;
                max-width: 400px;
                text-align: center;
                box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
            `;
            
            const actions = modal.querySelector('.confirmation-actions');
            actions.style.cssText = `
                margin-top: 20px;
                display: flex;
                gap: 10px;
                justify-content: center;
            `;
            
            document.body.appendChild(modal);
            
            const cancelBtn = modal.querySelector('#confirm-cancel');
            const okBtn = modal.querySelector('#confirm-ok');
            
            const cleanup = () => {
                if (modal.parentNode) {
                    modal.parentNode.removeChild(modal);
                }
            };
            
            cancelBtn.addEventListener('click', () => {
                cleanup();
                resolve(false);
            });
            
            okBtn.addEventListener('click', () => {
                cleanup();
                resolve(true);
            });
            
            // Close on overlay click
            overlay.addEventListener('click', (e) => {
                if (e.target === overlay) {
                    cleanup();
                    resolve(false);
                }
            });
            
            // Focus OK button
            okBtn.focus();
        });
    }
}

// Create and export a singleton instance
export const notificationService = new NotificationService(); 