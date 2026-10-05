import { AppEvent } from '../types/usage';

export function RecentEventsCard({ events }: { events: AppEvent[] }) {
  return <section className="event-panel">
    <h2>Recent activity</h2>
    {events.length ? <ul>{events.slice(0, 4).map((event, index) => <li key={index} title={event.description}>
      <span className={'event-marker' + (event.eventType === 'refresh_failed' ? ' event-error' : '')} />
      <span>{event.label}</span><time>{event.timestamp}</time>
    </li>)}</ul> : <p>No recent activity recorded.</p>}
  </section>;
}
