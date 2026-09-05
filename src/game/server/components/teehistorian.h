#ifndef GAME_SERVER_COMPONENTS_TEEHISTORIAN_H
#define GAME_SERVER_COMPONENTS_TEEHISTORIAN_H

#include "teehistorian.h"

#include <base/types.h>

#include <engine/console.h>

#include <game/server/server_component.h>
#include <game/server/teehistorian.h>

// TODO: Perhaps this should be refactor, but for now I’ll leave it as is.
class CTeeHistorianComponent : public CServerComponent
{
public:
	CTeeHistorianComponent();

	void OnInit(const void *pPersistentData) override;
	void OnConsoleInit(IConsole *pConsole) override;
	bool IsEnabled() override;
	void OnShutdown(void *pPersistentData) override;

	const char *GetComponentName() const override { return "TeeHistorian"; }

	CTeeHistorian *Recorder() { return &m_TeeHistorian; }
	bool Active() const { return m_Active; }

	// Recording phases, called by the game context at fixed points in the tick.
	void OnPreTick();
	void OnTickBegin(int Tick);
	void OnTickPlayersEnd();

	// Guarded wrappers around the recorder, called from the game context at the same
	// points as the previous direct recorder calls.
	void RecordConsoleCommand(int ClientId, int FlagMask, const char *pCmd, IConsole::IResult *pResult);
	void RecordPlayerTeam(int ClientId, int Team);
	void RecordTeamPractice(int Team, bool Practice);
	void RecordPlayerInput(int ClientId, uint32_t UniqueClientId, const CNetObj_PlayerInput *pInput);
	void RecordPlayerReady(int ClientId);
	void RecordAntibot(const void *pData, int DataSize);
	void RecordPlayerJoin(int ClientId, int Protocol);
	void RecordPlayerDrop(int ClientId, const char *pReason);
	void RecordPlayerRejoin(int ClientId);
	void RecordPlayerName(int ClientId, const char *pName);
	void RecordPlayerFinish(int ClientId, int TimeTicks);
	void RecordTeamFinish(int TeamId, int TimeTicks);
	void RecordAuthLogin(int ClientId, int Level, const char *pAuthName);
	void RecordAuthLogout(int ClientId);
	void RecordDDNetVersion(int ClientId, CUuid ConnectionId, int DDNetVersion, const char *pDDNetVersionStr);
	void RecordDDNetVersionOld(int ClientId, int DDNetVersion);
	void RecordPlayerMessage(int ClientId, const void *pMsg, int MsgSize);
	void RecordPlayerSwap(int ClientId1, int ClientId2);
	void RecordTeamSaveSuccess(int Team, CUuid SaveId, const char *pTeamSave);
	void RecordTeamSaveFailure(int Team);
	void RecordTeamLoadSuccess(int Team, CUuid SaveId, const char *pTeamSave);
	void RecordTeamLoadFailure(int Team);

private:
	static void TeeHistorianWrite(const void *pData, int DataSize, void *pUser);
	static void CommandCallback(int ClientId, int FlagMask, const char *pCmd, IConsole::IResult *pResult, void *pUser);

	CTeeHistorian m_TeeHistorian;
	ASYNCIO *m_pFile;
	bool m_Active;
};

#endif
