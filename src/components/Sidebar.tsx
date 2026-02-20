// 侧边栏导航组件
import { useState, useEffect } from 'react';
import { Box, Zap, Cpu, Activity, Server, MessageSquare } from 'lucide-react';
import { NavItem } from './NavItem';
import { useI18n } from '../hooks/useI18n';

declare const __APP_EDITION__: string;
const isFullEdition = __APP_EDITION__ === 'full';

export type PageType = 'models' | 'skills' | 'agents' | 'player' | 'logs' | 'chat';

interface SidebarProps {
    activePage: PageType;
    onPageChange: (page: PageType) => void;
    showLogsPage?: boolean;
}

export const Sidebar = ({ activePage, onPageChange, showLogsPage = true }: SidebarProps) => {
    const { t } = useI18n();
    // 轮询本地模型服务器状态
    const [serverRunning, setServerRunning] = useState(false);

    useEffect(() => {
        if (!isFullEdition) return;
        const check = async () => {
            if (window.electron?.getLocalModelServerStatus) {
                const status = await window.electron.getLocalModelServerStatus();
                setServerRunning(status.running);
            }
        };
        check();
        const interval = setInterval(check, 2000);
        return () => clearInterval(interval);
    }, []);

    return (
        <nav className="w-64 flex flex-col px-6 pb-6 relative z-[1]">
            <div className="font-bold mb-10 tracking-wide flex items-center gap-2">
                <img src={activePage === 'skills' ? './ico-yellow.svg' : './ico.svg'} alt="" className="w-6 h-6" />
                <span className={activePage === 'skills' ? 'text-cyber-warning' : 'text-cyber-accent'} style={{ fontFamily: "'JetBrains Mono', monospace", fontWeight: 700, letterSpacing: '1px', fontSize: '22px', lineHeight: 1 }}>CYBER NEXUS</span>
            </div>

            <div className="flex-1 space-y-6 text-sm">
                <NavItem
                    icon={<Box size={18} />}
                    label={t('nav.modelNexus')}
                    active={activePage === 'models'}
                    onClick={() => onPageChange('models')}
                />

                <NavItem
                    icon={<Zap size={18} />}
                    label={t('nav.skillBrowser')}
                    active={activePage === 'skills'}
                    onClick={() => onPageChange('skills')}
                    color="warning"
                />
                <NavItem
                    icon={<Cpu size={18} />}
                    label={t('nav.appManager')}
                    active={activePage === 'agents'}
                    onClick={() => onPageChange('agents')}
                />
                {isFullEdition && (
                    <NavItem
                        icon={<Server size={18} />}
                        label={t('nav.localServer')}
                        active={activePage === 'player'}
                        onClick={() => onPageChange('player')}
                    />
                )}
                {showLogsPage && (
                    <NavItem
                        icon={<Activity size={18} />}
                        label={t('nav.logsDebug')}
                        active={activePage === 'logs'}
                        onClick={() => onPageChange('logs')}
                    />
                )}
                <NavItem
                    icon={<MessageSquare size={18} />}
                    label={t('nav.channels')}
                    active={activePage === 'chat'}
                    onClick={() => onPageChange('chat')}
                />
            </div>

            {isFullEdition && (
                <div className="pt-4 text-[12px] text-cyber-text-secondary uppercase tracking-widest">
                    {t('nav.localServer')}: {serverRunning ? (
                        <span className="text-cyber-accent"><span style={{ fontSize: '8px', verticalAlign: '1px' }}>●</span> {t('status.running')}</span>
                    ) : (
                        <span className="text-cyber-text-muted/70"><span style={{ fontSize: '8px', verticalAlign: '1px' }}>○</span> {t('status.offline')}</span>
                    )}
                </div>
            )}
        </nav>
    );
};
