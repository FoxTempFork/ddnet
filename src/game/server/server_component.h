#ifndef GAME_SERVER_SERVER_COMPONENT_H
#define GAME_SERVER_SERVER_COMPONENT_H

// TODO: EDIT COMMENTS (COPPIED FROM CLIENT COMPONENT)

class CGameContext;
class CConfig;
class IServer;
class IStorage;
class IConsole;
class CCollision;

/**
 * This class is inherited by all the client components.
 *
 * These components can implement the virtual methods such as OnInit(), OnMessage(int Msg, void *pRawMsg) to provide their functionality.
 */
class CServerComponent
{
public:
	explicit CServerComponent();
	virtual ~CServerComponent() = default;

	void SetGameServer(CGameContext *pGameContext) { m_pGameServer = pGameContext; }

	/**
	 * Called to let the components register their console commands.
	 */
	virtual void OnConsoleInit(IConsole *pConsole) {}

	/**
	 * Called to let the components run initialization code.
	 */
	virtual void OnInit(const void *pPersistentData) {}

	/**
	 * Called every server tick.
	 * By default, 50 ticks per second.
	 */
	virtual void OnTick() {}

	virtual void OnSnap(int SnappingClient) {}

	/**
	 * Called to cleanup the component.
	 * This method is called when the client is closed.
	 */
	virtual void OnShutdown(void *pPersistentData) {}

	/**
	 * Component printable name
	 *
	 * @return component name
	 */
	virtual const char *GetComponentName() const = 0;

	/**
	 * Check if component enabled
	 *
	 * @return true if enabled
	 */
	virtual bool IsEnabled() { return m_Enabled; }

	/**
	 * Check if component disabled
	 *
	 * @return true if disabled
	 */
	virtual bool IsDisabled() { return !IsEnabled(); }

	/**
	 * Mark component as enabled or disabled
	 */
	virtual void SetEnabled(bool Value) { m_Enabled = Value; }

	/**
	 * Check if component dynamically plugged
	 *
	 * @return true if dynamically plugged
	 */
	virtual bool IsMarkedToDestroy() { return m_MarkedToDestroy; }

	/**
	 * Mark component as dynamic plugged
	 */
	virtual void MarkToDestroy() { m_MarkedToDestroy = true; }

protected:
	CGameContext *GameServer() const;
	CConfig *Config() const;
	IServer *Server() const;
	IStorage *Storage() const;
	IConsole *Console() const;
	CCollision *Collision() const;

private:
	bool m_Enabled;
	bool m_MarkedToDestroy;
	CGameContext *m_pGameServer = nullptr;
};

#endif
