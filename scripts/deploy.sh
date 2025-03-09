#!/bin/bash

# Colors for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Deploy willdolater service
deploy_willdolater() {
    log_info "Starting willdolater deployment"
    
    if [ ! -d "target/release" ]; then
        log_error "willdolater release build not found"
        return 1
    fi
    
    # Create backup of existing build
    if ssh "${SSH_OPTS:-}" "$USER@$HOST" "[ -d /opt/willdolater/release ]"; then
        local backup_name="/opt/willdolater/release.backup-$(date +%Y%m%d-%H%M%S)"
        log_info "Creating backup: $backup_name"
        ssh "${SSH_OPTS:-}" "$USER@$HOST" "sudo cp -r /opt/willdolater/release $backup_name"
    fi
    
    log_info "Ensuring directories exist"
    ssh "${SSH_OPTS:-}" "$USER@$HOST" "sudo mkdir -p /opt/willdolater/target/release /opt/willdolater/static"
    
    log_info "Preparing temporary directories"
    ssh "${SSH_OPTS:-}" "$USER@$HOST" "mkdir -p ~/willdolater_temp ~/willdolater_static_temp"
    
    log_info "Copying release build"
    # Find the executable files in the release directory
    executable_files=$(find target/release -maxdepth 1 -type f -executable -not -name "*.d" -not -name "*.rlib" -not -path "*/\.*")
    
    if [ -z "$executable_files" ]; then
        log_error "No executable files found in target/release"
        ls -la target/release/
        return 1
    fi
    
    log_info "Found executables: $executable_files"
    
    # Copy executable files
    scp "${SSH_OPTS:-}" "$executable_files" "$USER@$HOST:~/willdolater_temp/" || {
        log_error "Failed to copy willdolater executables to remote host"
        return 1
    }
    
    # Check if static directory exists
    if [ -d "static" ]; then
        log_info "Copying static files"
        scp "${SSH_OPTS:-}" -r static/* "$USER@$HOST:~/willdolater_static_temp/" || {
            log_error "Failed to copy static files to remote host"
            return 1
        }
    else
        log_warning "No static directory found, skipping static files"
    fi
    
    log_info "Moving build to final destination"
    ssh "${SSH_OPTS:-}" "$USER@$HOST" "sudo rm -rf /opt/willdolater/target/release/* && \
                       sudo mv ~/willdolater_temp/* /opt/willdolater/target/release/ && \
                       sudo rm -rf /opt/willdolater/static/* && \
                       sudo mv ~/willdolater_static_temp/* /opt/willdolater/static/ 2>/dev/null || true && \
                       sudo chown -R willdolater:willdolater /opt/willdolater" || {
        log_error "Failed to move build to destination"
        return 1
    }
    
    log_info "Restarting willdolater service"
    if ssh "${SSH_OPTS:-}" "$USER@$HOST" "sudo systemctl restart willdolater"; then
        log_success "Successfully restarted willdolater service"
    else
        log_error "Failed to restart willdolater service"
        return 1
    fi
    
    log_info "Verifying willdolater service status"
    if ssh "${SSH_OPTS:-}" "$USER@$HOST" "sudo systemctl is-active willdolater"; then
        log_success "willdolater service is running"
    else
        log_error "willdolater service failed to start"
        return 1
    fi
    
    log_success "Completed willdolater deployment"
}

# Main execution
if [ -z "$USER" ] || [ -z "$HOST" ]; then
    log_error "Environment variables USER and HOST must be set"
    exit 1
fi

deploy_willdolater
