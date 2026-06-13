import { describe, it, expect, beforeEach } from 'vitest';
import { useAuthStore } from '../store/authStore';

describe('authStore', () => {
  beforeEach(() => {
    useAuthStore.setState({ token: null, teamId: null });
    localStorage.clear();
  });

  it('is not authenticated initially', () => {
    expect(useAuthStore.getState().isAuthenticated()).toBe(false);
  });

  it('login sets token and teamId', () => {
    useAuthStore.getState().login('my-token', 'team-123');
    const state = useAuthStore.getState();
    expect(state.token).toBe('my-token');
    expect(state.teamId).toBe('team-123');
    expect(state.isAuthenticated()).toBe(true);
  });

  it('login writes to localStorage', () => {
    useAuthStore.getState().login('tok', 'tid');
    expect(localStorage.getItem('kc_access_token')).toBe('tok');
  });

  it('logout clears token', () => {
    useAuthStore.getState().login('tok', 'tid');
    useAuthStore.getState().logout();
    expect(useAuthStore.getState().token).toBeNull();
    expect(useAuthStore.getState().isAuthenticated()).toBe(false);
  });

  it('logout removes from localStorage', () => {
    useAuthStore.getState().login('tok', 'tid');
    useAuthStore.getState().logout();
    expect(localStorage.getItem('kc_access_token')).toBeNull();
  });
});
