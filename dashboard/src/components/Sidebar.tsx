import { NavLink } from 'react-router-dom';
import {
  LayoutDashboard,
  PlayCircle,
  DollarSign,
  Timer,
  Bot,
  FlaskConical,
  MessageSquare,
  Users,
  CreditCard,
  LogOut,
} from 'lucide-react';
import { useAuthStore } from '../store/authStore';

const nav = [
  { to: '/', label: 'Overview', icon: LayoutDashboard, end: true },
  { to: '/runs', label: 'Runs', icon: PlayCircle },
  { to: '/cost', label: 'Cost', icon: DollarSign },
  { to: '/latency', label: 'Latency', icon: Timer },
  { to: '/agents', label: 'Agents', icon: Bot },
  { to: '/evaluations', label: 'Evaluations', icon: FlaskConical },
  { to: '/prompts', label: 'Prompts', icon: MessageSquare },
  { to: '/team', label: 'Team', icon: Users },
  { to: '/billing', label: 'Billing', icon: CreditCard },
];

export function Sidebar() {
  const logout = useAuthStore((s) => s.logout);

  return (
    <aside className="flex h-full w-56 flex-col border-r border-gray-200 bg-white">
      {/* Logo */}
      <div className="flex h-14 items-center gap-2 border-b border-gray-200 px-4">
        <div className="flex h-7 w-7 items-center justify-center rounded-md bg-brand-600 text-xs font-bold text-white">
          K
        </div>
        <span className="text-sm font-semibold text-gray-900">Kainetic Cloud</span>
      </div>

      {/* Navigation */}
      <nav className="flex-1 overflow-y-auto p-2" aria-label="Primary navigation">
        <ul className="space-y-0.5">
          {nav.map(({ to, label, icon: Icon, end }) => (
            <li key={to}>
              <NavLink
                to={to}
                end={end}
                className={({ isActive }) =>
                  `flex items-center gap-2.5 rounded-md px-3 py-2 text-sm font-medium transition-colors ${
                    isActive
                      ? 'bg-brand-50 text-brand-700'
                      : 'text-gray-600 hover:bg-gray-100 hover:text-gray-900'
                  }`
                }
              >
                <Icon className="h-4 w-4" />
                {label}
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>

      {/* Logout */}
      <div className="border-t border-gray-200 p-2">
        <button
          onClick={logout}
          className="flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm font-medium text-gray-600 transition-colors hover:bg-gray-100 hover:text-gray-900"
        >
          <LogOut className="h-4 w-4" />
          Sign out
        </button>
      </div>
    </aside>
  );
}
