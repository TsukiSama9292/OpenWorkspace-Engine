import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import LoginPage from '../routes/login/+page.svelte';

describe('login page', () => {
  it('offers no dead ends — every button does something', () => {
    render(LoginPage);
    expect(screen.getByRole('button', { name: /Continue/ })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /SSO|Single Sign-On|SSH/ })).toBeNull();
  });

  it('shows a plain-words error on rejected credentials', async () => {
    render(LoginPage);
    await fireEvent.input(screen.getByLabelText('Account'), { target: { value: 'user' } });
    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'wrong' } });
    const form = screen.getByRole('button', { name: /Continue/ }).closest('form');
    if (!form) throw new Error('login form not found');
    await fireEvent.submit(form);
    await waitFor(() => expect(screen.getByText('Invalid credentials')).toBeTruthy());
  });
});
