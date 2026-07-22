import { Clock } from 'lucide-react';

interface EventItem {
  type: 'session_started' | 'session_ended' | 'usage_updated' | 'limit_updated' | 'monitoring_paused' | 'monitoring_resumed' | 'refresh_failed' | 'reset_detected';
  label: string;
  timestamp: string;
  description?: string;
}

interface RecentEventsCardProps {
  events: EventItem[];
}

const eventColors: Record<string, string> = {
  session_started: 'var(--green)',
  session_ended: 'var(--text-muted)',
  usage_updated: 'var(--blue)',
  limit_updated: 'var(--purple)',
  monitoring_paused: 'var(--warning)',
  monitoring_resumed: 'var(--green)',
  refresh_failed: 'var(--danger)',
  reset_detected: 'var(--blue)',
};

export function RecentEventsCard({ events }: RecentEventsCardProps) {
  return (
    <div
      className="rounded-xl p-5"
      style={{
        background: 'var(--bg-card)',
        border: '1px solid var(--border-default)',
      }}
    >
      <h3 className="text-[15px] font-semibold mb-4" style={{ color: 'var(--text-primary)' }}>
        Recent Events
      </h3>

      {events.length > 0 ? (
        <div className="space-y-3">
          {events.slice(0, 5).map((event, idx) => (
            <div key={idx} className="flex items-center gap-3">
              <div
                className="w-2 h-2 rounded-full shrink-0"
                style={{ background: eventColors[event.type] || 'var(--text-muted)' }}
              />
              <div className="flex items-center gap-2 flex-1 min-w-0">
                <span className="text-[13px] font-medium" style={{ color: 'var(--text-primary)' }}>
                  {event.label}
                </span>
                <span className="text-[11px]" style={{ color: 'var(--text-muted)' }}>
                  {event.timestamp}
                </span>
              </div>
              {event.description && (
                <span className="text-[11px] shrink-0" style={{ color: 'var(--text-muted)' }}>
                  {event.description}
                </span>
              )}
            </div>
          ))}
        </div>
      ) : (
        <div className="flex flex-col items-center justify-center py-6">
          <Clock size={20} style={{ color: 'var(--text-faint)', marginBottom: 8 }} />
          <span className="text-sm" style={{ color: 'var(--text-muted)' }}>
            No recent events
          </span>
          <span className="text-[11px] mt-1" style={{ color: 'var(--text-faint)' }}>
            Events will appear as monitoring activity occurs
          </span>
        </div>
      )}
    </div>
  );
}

export type { EventItem };
