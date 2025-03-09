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
    if ssh "$USER@$HOST" "[ -d /opt/willdolater/release ]"; then
        local backup_name="/opt/willdolater/release.backup-$(date +%Y%m%d-%H%M%S)"
        log_info "Creating backup: $backup_name"
        ssh "$USER@$HOST" "sudo cp -r /opt/willdolater/release $backup_name"
    fi
    
    log_info "Ensuring directory exists"
    ssh "$USER@$HOST" "sudo mkdir -p /opt/willdolater/target/release"
    
    log_info "Copying release build"
    scp -C -c aes128-gcm@openssh.com -r target/release/* "$USER@$HOST:~/willdolater_temp/" || {
        log_error "Failed to copy willdolater build to remote host"
        return 1
    }
    
    log_info "Moving build to final destination"
    ssh "$USER@$HOST" "sudo rm -rf /opt/willdolater/target/release/* && \
                       sudo mv ~/willdolater_temp/* /opt/willdolater/target/release/ && \
                       sudo chown -R willdolater:willdolater /opt/willdolater" || {
        log_error "Failed to move build to destination"
        return 1
    }
    
    log_info "Restarting willdolater service"
    if ssh "$USER@$HOST" "sudo systemctl restart willdolater"; then
        log_success "Successfully restarted willdolater service"
    else
        log_error "Failed to restart willdolater service"
        return 1
    fi
    
    log_info "Verifying willdolater service status"
    if ssh "$USER@$HOST" "sudo systemctl is-active willdolater"; then
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
