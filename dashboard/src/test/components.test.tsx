import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { RunStatusBadge } from '../components/RunStatusBadge';
import { StatCard } from '../components/StatCard';
import { ErrorMessage } from '../components/ErrorMessage';
import { Spinner } from '../components/Spinner';

describe('RunStatusBadge', () => {
  it('renders completed status', () => {
    render(<RunStatusBadge status="completed" />);
    expect(screen.getByText('completed')).toBeInTheDocument();
  });

  it('renders failed status', () => {
    render(<RunStatusBadge status="failed" />);
    expect(screen.getByText('failed')).toBeInTheDocument();
  });

  it('renders running status', () => {
    render(<RunStatusBadge status="running" />);
    expect(screen.getByText('running')).toBeInTheDocument();
  });
});

describe('StatCard', () => {
  it('renders title and value', () => {
    render(<StatCard title="Total Runs" value={42} />);
    expect(screen.getByText('Total Runs')).toBeInTheDocument();
    expect(screen.getByText(42)).toBeInTheDocument();
  });

  it('renders sub text when provided', () => {
    render(<StatCard title="Cost" value="$0.01" sub="+5% vs last week" />);
    expect(screen.getByText('+5% vs last week')).toBeInTheDocument();
  });
});

describe('ErrorMessage', () => {
  it('renders error text', () => {
    render(<ErrorMessage message="Something went wrong" />);
    expect(screen.getByText('Something went wrong')).toBeInTheDocument();
  });

  it('renders custom title', () => {
    render(<ErrorMessage message="oops" title="Request Failed" />);
    expect(screen.getByText('Request Failed')).toBeInTheDocument();
  });
});

describe('Spinner', () => {
  it('renders without crashing', () => {
    render(<Spinner />);
    expect(screen.getByLabelText('Loading')).toBeInTheDocument();
  });
});
